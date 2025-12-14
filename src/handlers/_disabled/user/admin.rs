//! Admin user management handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use uuid::Uuid;
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::auth::UserInfo;
use crate::error::{ApiError, Result};
use crate::handlers::authorization::require_admin;
use crate::AppState;

use super::types::{log_user_activity, AdminUpdateUserRequest};

/// Admin: Update any user (requires admin role)
#[utoipa::path(
    put,
    path = "/api/users/{id}",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID to update")
    ),
    request_body = AdminUpdateUserRequest,
    responses(
        (status = 200, description = "User updated successfully", body = UserInfo),
        (status = 400, description = "Validation error or invalid role"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn admin_update_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    user: AuthenticatedUser,
    Json(request): Json<AdminUpdateUserRequest>,
) -> Result<Json<UserInfo>> {
    // Check admin permissions
    require_admin(&user.0)?;

    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Validate role if provided
    if let Some(role) = &request.role {
        crate::auth::Role::from_str(role)
            .map_err(|_| ApiError::BadRequest("Invalid role".to_string()))?;
    }

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
    if request.role.is_some() {
        query_parts.push(format!("role = ${}", param_count));
        param_count += 1;
    }
    if request.is_active.is_some() {
        query_parts.push(format!("is_active = ${}", param_count));
        param_count += 1;
    }
    if request.wallet_address.is_some() {
        query_parts.push(format!("wallet_address = ${}", param_count));
        param_count += 1;
    }
    if request.blockchain_registered.is_some() {
        query_parts.push(format!("blockchain_registered = ${}", param_count));
        param_count += 1;
    }

    if query_parts.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".to_string()));
    }

    query_parts.push("updated_at = NOW()".to_string());
    let query = format!(
        "UPDATE users SET {} WHERE id = ${}",
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
    if let Some(role) = &request.role {
        query_builder = query_builder.bind(role);
    }
    if let Some(is_active) = request.is_active {
        query_builder = query_builder.bind(is_active);
    }
    if let Some(wallet_address) = &request.wallet_address {
        query_builder = query_builder.bind(wallet_address);
    }
    if let Some(blockchain_registered) = request.blockchain_registered {
        query_builder = query_builder.bind(blockchain_registered);
    }

    query_builder = query_builder.bind(user_id);

    let result = query_builder
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to update user: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log admin action
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "admin_user_updated".to_string(),
        Some(serde_json::json!({
            "target_user_id": user_id,
            "changes": request
        })),
        None,
        None,
    )
    .await;

    // Return updated user info
    crate::handlers::auth::get_user(State(state), Path(user_id)).await
}

/// Admin: Deactivate user (soft delete)
#[utoipa::path(
    post,
    path = "/api/users/{id}/deactivate",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID to deactivate")
    ),
    responses(
        (status = 204, description = "User deactivated successfully"),
        (status = 400, description = "Cannot deactivate own account"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn admin_deactivate_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    user: AuthenticatedUser,
) -> Result<StatusCode> {
    // Check admin permissions
    require_admin(&user.0)?;

    // Cannot deactivate self
    if user_id == user.0.sub {
        return Err(ApiError::BadRequest(
            "Cannot deactivate your own account".to_string(),
        ));
    }

    // Deactivate user
    let result =
        sqlx::query("UPDATE users SET is_active = false, updated_at = NOW() WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to deactivate user: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log admin action
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "admin_user_deactivated".to_string(),
        Some(serde_json::json!({
            "target_user_id": user_id
        })),
        None,
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Admin: Reactivate user
#[utoipa::path(
    post,
    path = "/api/users/{id}/reactivate",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID to reactivate")
    ),
    responses(
        (status = 204, description = "User reactivated successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn admin_reactivate_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    user: AuthenticatedUser,
) -> Result<StatusCode> {
    // Check admin permissions
    require_admin(&user.0)?;

    // Reactivate user
    let result = sqlx::query("UPDATE users SET is_active = true, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to reactivate user: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log admin action
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "admin_user_reactivated".to_string(),
        Some(serde_json::json!({
            "target_user_id": user_id
        })),
        None,
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
