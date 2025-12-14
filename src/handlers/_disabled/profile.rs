use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::auth::password::PasswordService;
use crate::auth::UserInfo;
use crate::error::{ApiError, Result};
use crate::services::AuditEvent;
use crate::utils::extract_ip_address;
use crate::AppState;

use super::UserRow;

/// User profile update request
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct UpdateProfileRequest {
    #[validate(email)]
    #[schema(example = "john.doe@example.com")]
    pub email: Option<String>,

    #[validate(length(min = 1, max = 100))]
    #[schema(example = "John")]
    pub first_name: Option<String>,

    #[validate(length(min = 1, max = 100))]
    #[schema(example = "Doe")]
    pub last_name: Option<String>,

    #[validate(length(min = 32, max = 44))]
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub wallet_address: Option<String>,
}

/// Password change request
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 8, max = 128))]
    #[schema(example = "OldPassword123!")]
    pub current_password: String,

    #[validate(length(min = 8, max = 128))]
    #[schema(example = "NewSecurePassword456!")]
    pub new_password: String,
}

/// Get current user profile
#[utoipa::path(
    get,
    path = "/api/auth/profile",
    tag = "auth",
    responses(
        (status = 200, description = "User profile retrieved successfully", body = UserInfo),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<UserInfo>> {
    let user_data = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, blockchain_registered,
                is_active, email_verified, created_at, updated_at
         FROM users 
         WHERE id = $1 AND is_active = true",
    )
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let user_data = user_data.ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let profile = UserInfo {
        id: user_data.id,
        username: user_data.username,
        email: user_data.email,
        role: user_data.role,
        wallet_address: user_data.wallet_address,
    };

    Ok(Json(profile))
}

/// Update user profile
#[utoipa::path(
    post,
    path = "/api/auth/profile",
    tag = "auth",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Profile updated successfully", body = UserInfo),
        (status = 400, description = "Validation error or no fields to update"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<UserInfo>> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Build dynamic update query
    let mut query_parts = Vec::new();
    let mut param_count = 1;

    if request.email.is_some() {
        query_parts.push(format!("email = ${}", param_count));
        param_count += 1;
    }
    if request.first_name.is_some() {
        query_parts.push(format!("first_name = ${}", param_count));
        param_count += 1;
    }
    if request.last_name.is_some() {
        query_parts.push(format!("last_name = ${}", param_count));
        param_count += 1;
    }
    if request.wallet_address.is_some() {
        query_parts.push(format!("wallet_address = ${}", param_count));
        param_count += 1;
    }

    if query_parts.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".to_string()));
    }

    query_parts.push("updated_at = NOW()".to_string());
    let query = format!(
        "UPDATE users SET {} WHERE id = ${} AND is_active = true",
        query_parts.join(", "),
        param_count
    );

    let mut query_builder = sqlx::query(&query);

    if let Some(email) = &request.email {
        query_builder = query_builder.bind(email);
    }
    if let Some(first_name) = &request.first_name {
        query_builder = query_builder.bind(first_name);
    }
    if let Some(last_name) = &request.last_name {
        query_builder = query_builder.bind(last_name);
    }
    if let Some(wallet_address) = &request.wallet_address {
        query_builder = query_builder.bind(wallet_address);
    }

    query_builder = query_builder.bind(user.0.sub);

    let result = query_builder
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to update profile: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Return updated profile
    get_profile(State(state), user).await
}

/// Change password
#[utoipa::path(
    post,
    path = "/api/auth/password",
    tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed successfully"),
        (status = 400, description = "Validation error or incorrect current password"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<StatusCode> {
    // Extract IP for audit logging
    let ip_address = extract_ip_address(&headers);

    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Get current password hash
    let current_hash = sqlx::query_scalar::<_, String>(
        "SELECT password_hash FROM users WHERE id = $1 AND is_active = true",
    )
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let current_hash =
        current_hash.ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    // Verify current password
    let password_valid =
        PasswordService::verify_password(&request.current_password, &current_hash)?;
    if !password_valid {
        return Err(ApiError::BadRequest(
            "Current password is incorrect".to_string(),
        ));
    }

    // Hash new password
    let new_password_hash = PasswordService::hash_password(&request.new_password)?;

    // Update password
    let result = sqlx::query(
        "UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2 AND is_active = true"
    )
    .bind(&new_password_hash)
    .bind(user.0.sub)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to update password: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log password change
    state.audit_logger.log_async(AuditEvent::PasswordChanged {
        user_id: user.0.sub,
        ip: ip_address,
    });

    Ok(StatusCode::NO_CONTENT)
}
