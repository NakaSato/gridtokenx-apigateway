use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;
use utoipa::ToSchema;

use crate::auth::UserInfo;
use crate::auth::middleware::AuthenticatedUser;
use crate::auth::password::PasswordService;
use crate::error::{ApiError, Result};
use crate::AppState;

/// user registration request with additional validation
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 50))]
    #[schema(example = "john_doe")]
    pub username: String,
    
    #[validate(email)]
    #[schema(example = "john.doe@example.com")]
    pub email: String,
    
    #[validate(length(min = 8, max = 128))]
    #[schema(example = "SecurePassword123!")]
    pub password: String,
    
    #[validate(length(min = 1, max = 20))]
    #[schema(example = "user")]
    pub role: String,
    
    #[validate(length(min = 1, max = 100))]
    #[schema(example = "John")]
    pub first_name: String,
    
    #[validate(length(min = 1, max = 100))]
    #[schema(example = "Doe")]
    pub last_name: String,
    
    // wallet_address removed - assigned after email verification via /api/users/wallet endpoint
}

/// Wallet address management request
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateWalletRequest {
    #[validate(length(min = 32, max = 44))]
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub wallet_address: String,
    
    pub verify_ownership: Option<bool>,
}

/// Admin user update request
#[derive(Debug, Deserialize, Validate, Serialize, ToSchema)]
pub struct AdminUpdateUserRequest {
    #[validate(email)]
    pub email: Option<String>,
    
    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,
    
    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,
    
    #[validate(length(min = 1, max = 20))]
    pub role: Option<String>,
    
    pub is_active: Option<bool>,
    
    #[validate(length(min = 32, max = 44))]
    pub wallet_address: Option<String>,
    
    pub blockchain_registered: Option<bool>,
}

/// User activity log entry
#[derive(Debug, Serialize, ToSchema)]
pub struct UserActivity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub action: String,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// User activity response
#[derive(Debug, Serialize, ToSchema)]
pub struct UserActivityResponse {
    pub activities: Vec<UserActivity>,
    pub total: u64,
}

/// Registration response (without JWT - pending email verification)
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterResponse {
    pub message: String,
    pub user: BasicUserInfo,
    pub email_verification_sent: bool,
    pub verification_required: bool,
}

/// Basic user information for registration response
#[derive(Debug, Serialize, ToSchema)]
pub struct BasicUserInfo {
    pub username: String,
    pub email: String,
    pub role: String,
}

/// Enhanced user registration with email verification
#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered successfully", body = RegisterResponse),
        (status = 400, description = "Validation error or user already exists"),
        (status = 500, description = "Failed to send verification email")
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>)> {
    // Validate request
    request.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Validate role
    crate::auth::Role::from_str(&request.role)
        .map_err(|_| ApiError::BadRequest("Invalid role".to_string()))?;

    // Check if username already exists
    let existing_user = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE username = $1 OR email = $2"
    )
    .bind(&request.username)
    .bind(&request.email)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if existing_user > 0 {
        return Err(ApiError::BadRequest("Username or email already exists".to_string()));
    }

    // Hash password
    let password_hash = PasswordService::hash_password(&request.password)?;

    // Create user with enhanced fields (email_verified = false by default)
    // wallet_address is NULL initially - assigned after email verification
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role,
                           first_name, last_name, is_active, 
                           email_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, ($5)::user_role, $6, $7, true, false, NOW(), NOW())"
    )
    .bind(user_id)
    .bind(&request.username)
    .bind(&request.email)
    .bind(&password_hash)
    .bind(&request.role)
    .bind(&request.first_name)
    .bind(&request.last_name)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to create user: {}", e)))?;

    // Generate verification token
    let token = crate::services::TokenService::generate_verification_token();
    let token_hash = crate::services::TokenService::hash_token(&token);
    
    // Calculate expiration time from config
    let expiry_hours = state.config.email.verification_expiry_hours;
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(expiry_hours);

    // Store hashed token in database
    sqlx::query(
        "UPDATE users SET 
         email_verification_token = $1,
         email_verification_sent_at = NOW(),
         email_verification_expires_at = $2
         WHERE id = $3"
    )
    .bind(&token_hash)
    .bind(expires_at)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to store verification token: {}", e)))?;

    // Send verification email if email service is available
    let email_sent = if let Some(email_service) = &state.email_service {
        match email_service
            .send_verification_email(&request.email, &token, &request.username)
            .await
        {
            Ok(_) => {
                // Log successful email send
                let _ = log_user_activity(
                    &state.db,
                    user_id,
                    "email_verification_sent".to_string(),
                    Some(serde_json::json!({
                        "email": request.email,
                        "expires_at": expires_at
                    })),
                    None,
                    None,
                ).await;
                true
            }
            Err(e) => {
                // Log failed email send but don't fail registration
                tracing::error!("Failed to send verification email: {}", e);
                let _ = log_user_activity(
                    &state.db,
                    user_id,
                    "email_verification_send_failed".to_string(),
                    Some(serde_json::json!({
                        "email": request.email,
                        "error": e.to_string()
                    })),
                    None,
                    None,
                ).await;
                false
            }
        }
    } else {
        tracing::warn!("Email service not configured, skipping verification email");
        false
    };

    // Log user registration activity
    let _ = log_user_activity(
        &state.db,
        user_id,
        "user_registered".to_string(),
        Some(serde_json::json!({
            "role": request.role,
            "email_verification_sent": email_sent
        })),
        None,
        None,
    ).await;

    // Return registration response (NO JWT - user must verify email first)
    let response = RegisterResponse {
        message: if email_sent {
            "Registration successful! Please check your email to verify your account.".to_string()
        } else {
            "Registration successful! Email verification is pending. Please contact support if you don't receive the email.".to_string()
        },
        user: BasicUserInfo {
            username: request.username,
            email: request.email,
            role: request.role,
        },
        email_verification_sent: email_sent,
        verification_required: state.config.email.verification_required,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// Update wallet address for current user
#[utoipa::path(
    post,
    path = "/api/user/wallet",
    tag = "users",
    request_body = UpdateWalletRequest,
    responses(
        (status = 200, description = "Wallet address updated successfully", body = UserInfo),
        (status = 400, description = "Invalid wallet address format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_wallet_address(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(request): Json<UpdateWalletRequest>,
) -> Result<Json<UserInfo>> {
    // Validate request
    request.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Check if email is verified (required before wallet connection)
    let user_verified = sqlx::query_scalar::<_, bool>(
        "SELECT email_verified FROM users WHERE id = $1"
    )
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if !user_verified {
        return Err(ApiError::email_not_verified());
    }

    // Validate wallet address format
    if !is_valid_solana_address(&request.wallet_address) {
        return Err(ApiError::BadRequest("Invalid Solana wallet address format".to_string()));
    }

    // Check if wallet address is already in use
    let existing_wallet = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE wallet_address = $1 AND id != $2"
    )
    .bind(&request.wallet_address)
    .bind(user.0.sub)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if existing_wallet > 0 {
        return Err(ApiError::BadRequest("Wallet address is already in use".to_string()));
    }

    // Update wallet address
    let result = sqlx::query(
        "UPDATE users SET wallet_address = $1, updated_at = NOW() WHERE id = $2 AND is_active = true"
    )
    .bind(&request.wallet_address)
    .bind(user.0.sub)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to update wallet address: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log wallet update activity
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "wallet_updated".to_string(),
        Some(serde_json::json!({
            "wallet_address": request.wallet_address
        })),
        None,
        None,
    ).await;

    // Return updated profile
    crate::handlers::auth::get_profile(State(state), user).await
}

/// Remove wallet address for current user
#[utoipa::path(
    delete,
    path = "/api/user/wallet",
    tag = "users",
    responses(
        (status = 204, description = "Wallet address removed successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn remove_wallet_address(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<StatusCode> {
    // Update wallet address to null
    let result = sqlx::query(
        "UPDATE users SET wallet_address = NULL, blockchain_registered = false, updated_at = NOW() 
         WHERE id = $1 AND is_active = true"
    )
    .bind(user.0.sub)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to remove wallet address: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log wallet removal activity
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "wallet_removed".to_string(),
        None,
        None,
        None,
    ).await;

    Ok(StatusCode::NO_CONTENT)
}

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
    if !user.0.has_any_role(&["admin"]) {
        return Err(ApiError::Authorization("Admin access required".to_string()));
    }

    // Validate request
    request.validate()
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
        query_parts.push(format!("role = (${}::text)::user_role", param_count));
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
    ).await;

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
    if !user.0.has_any_role(&["admin"]) {
        return Err(ApiError::Authorization("Admin access required".to_string()));
    }

    // Cannot deactivate self
    if user_id == user.0.sub {
        return Err(ApiError::BadRequest("Cannot deactivate your own account".to_string()));
    }

    // Deactivate user
    let result = sqlx::query(
        "UPDATE users SET is_active = false, updated_at = NOW() WHERE id = $1"
    )
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
    ).await;

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
    if !user.0.has_any_role(&["admin"]) {
        return Err(ApiError::Authorization("Admin access required".to_string()));
    }

    // Reactivate user
    let result = sqlx::query(
        "UPDATE users SET is_active = true, updated_at = NOW() WHERE id = $1"
    )
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
    ).await;

    Ok(StatusCode::NO_CONTENT)
}

/// Get user activity log (admin only)
#[utoipa::path(
    get,
    path = "/api/users/{id}/activity",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID to get activity for"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "User activity retrieved successfully", body = UserActivityResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required or can only view own activity"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_user_activity(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(params): Query<ActivityQuery>,
    user: AuthenticatedUser,
) -> Result<Json<ActivityListResponse>> {
    // Check admin permissions or self-access
    if !user.0.has_any_role(&["admin"]) && user_id != user.0.sub {
        return Err(ApiError::Authorization("Admin access required or can only view own activity".to_string()));
    }

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * per_page;

    // Get activities
    let activities = sqlx::query_as::<_, ActivityRow>(
        "SELECT id, user_id, action, details, ip_address, user_agent, created_at
         FROM user_activities 
         WHERE user_id = $1 
         ORDER BY created_at DESC 
         LIMIT $2 OFFSET $3"
    )
    .bind(user_id)
    .bind(per_page as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Get total count
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_activities WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let activity_list: Vec<UserActivity> = activities
        .into_iter()
        .map(|row| UserActivity {
            id: row.id,
            user_id: row.user_id,
            action: row.action,
            details: row.details,
            ip_address: row.ip_address,
            user_agent: row.user_agent,
            created_at: row.created_at,
        })
        .collect();

    let total_pages = ((total as u32) + per_page - 1) / per_page;

    let response = ActivityListResponse {
        activities: activity_list,
        total: total as u64,
        page,
        per_page,
        total_pages,
    };

    Ok(Json(response))
}

// Helper functions

async fn log_user_activity(
    db: &sqlx::PgPool,
    user_id: Uuid,
    action: String,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> Result<()> {
    let activity_id = Uuid::new_v4();
    
    let _ = sqlx::query(
        "INSERT INTO user_activities (id, user_id, action, details, ip_address, user_agent, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())"
    )
    .bind(activity_id)
    .bind(user_id)
    .bind(action)
    .bind(details)
    .bind(ip_address)
    .bind(user_agent)
    .execute(db)
    .await;

    Ok(())
}

fn is_valid_solana_address(address: &str) -> bool {
    // Basic Solana address validation (base58, 32-44 characters)
    if address.len() < 32 || address.len() > 44 {
        return false;
    }
    
    // Check if it's valid base58
    bs58::decode(address).into_vec().is_ok()
}

// Additional helper types

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActivityQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityListResponse {
    pub activities: Vec<UserActivity>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    id: Uuid,
    user_id: Uuid,
    action: String,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}