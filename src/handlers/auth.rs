use axum::{
    extract::{Path, Query, State},
    http::{StatusCode, HeaderMap},
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;
use utoipa::ToSchema;

use crate::auth::{SecureAuthResponse, Claims, UserInfo, SecureUserInfo};
use crate::auth::middleware::AuthenticatedUser;
use crate::auth::password::PasswordService;
use crate::error::{ApiError, Result};
use crate::services::AuditEvent;
use crate::utils::{extract_ip_address, extract_user_agent};
use crate::AppState;

/// Login request
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 50))]
    #[schema(example = "john_doe", min_length = 3, max_length = 50)]
    pub username: String,
    
    #[validate(length(min = 8, max = 128))]
    #[schema(example = "SecurePassword123!", min_length = 8, max_length = 128)]
    pub password: String,
}

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

/// User search/filter query parameters
#[derive(Debug, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct UserSearchQuery {
    /// Search term for username, email, first name, or last name
    pub search: Option<String>,
    
    /// Filter by role
    pub role: Option<String>,
    
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    
    /// Number of items per page (max 100)
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    
    /// Sort field: "created_at", "username", "email"
    pub sort_by: Option<String>,
    
    /// Sort direction: "asc" or "desc"
    #[serde(default = "default_sort_order")]
    pub sort_order: crate::utils::SortOrder,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

fn default_sort_order() -> crate::utils::SortOrder {
    crate::utils::SortOrder::Desc
}

impl UserSearchQuery {
    /// Validate and normalize parameters
    pub fn validate(&mut self) -> Result<()> {
        // Ensure page is at least 1
        if self.page < 1 {
            self.page = 1;
        }
        
        // Limit page size to 100
        if self.page_size < 1 {
            self.page_size = 20;
        } else if self.page_size > 100 {
            self.page_size = 100;
        }
        
        // Validate sort field
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "created_at" | "username" | "email" | "role" => {}
                _ => return Err(ApiError::validation_error(
                    "Invalid sort_by field. Allowed values: created_at, username, email, role",
                    Some("sort_by"),
                )),
            }
        }
        
        Ok(())
    }
    
    /// Calculate SQL LIMIT value
    pub fn limit(&self) -> i64 {
        self.page_size as i64
    }
    
    /// Calculate SQL OFFSET value
    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }
    
    /// Get sort direction as SQL string
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            crate::utils::SortOrder::Asc => "ASC",
            crate::utils::SortOrder::Desc => "DESC",
        }
    }
    
    /// Get sort field with default
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("created_at")
    }
}

/// Paginated user response
#[derive(Debug, Serialize, ToSchema)]
pub struct UserListResponse {
    pub data: Vec<UserInfo>,
    pub pagination: crate::utils::PaginationMeta,
}

/// Login handler
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = SecureAuthResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "Email not verified"),
        (status = 400, description = "Validation error")
    )
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Json<SecureAuthResponse>> {
    // Extract IP and user-agent for audit logging
    let ip_address = extract_ip_address(&headers);
    let user_agent = extract_user_agent(&headers);

    // Validate request
    request.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Find user by username
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, blockchain_registered,
                is_active, email_verified, created_at, updated_at
         FROM users 
         WHERE username = $1 AND is_active = true"
    )
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let user = match user {
        Some(u) => u,
        None => {
            // Log failed login attempt
            state.audit_logger.log_async(AuditEvent::LoginFailed {
                email: request.username.clone(),
                ip: ip_address,
                reason: "User not found".to_string(),
                user_agent,
            });
            return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
        }
    };

    // Verify password
    let password_valid = PasswordService::verify_password(&request.password, &user.password_hash)?;
    if !password_valid {
        // Log failed login attempt
        state.audit_logger.log_async(AuditEvent::LoginFailed {
            email: user.email.clone(),
            ip: ip_address,
            reason: "Invalid password".to_string(),
            user_agent,
        });
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    // Check email verification if required (bypass in test mode)
    if state.config.email.verification_required && !user.email_verified && !state.config.test_mode {
        // Log failed login due to unverified email
        state.audit_logger.log_async(AuditEvent::LoginFailed {
            email: user.email.clone(),
            ip: ip_address.clone(),
            reason: "Email not verified".to_string(),
            user_agent: user_agent.clone(),
        });
        return Err(ApiError::Forbidden(
            "Email not verified. Please check your email for verification link.".to_string()
        ));
    }

    // Create JWT claims
    let claims = Claims::new(user.id, user.username.clone(), user.role.clone());
    
    // Generate token
    let access_token = state.jwt_service.encode_token(&claims)?;

    // Update last login
    let _ = sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await;

    // Log successful login
    state.audit_logger.log_async(AuditEvent::UserLogin {
        user_id: user.id,
        ip: ip_address,
        user_agent,
    });

    let response = SecureAuthResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60, // 24 hours in seconds
        user: SecureUserInfo {
            username: user.username,
            email: user.email,
            role: user.role,
            blockchain_registered: user.wallet_address.is_some(),
        },
    };

    Ok(Json(response))
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
         WHERE id = $1 AND is_active = true"
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
    request.validate()
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
    request.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Get current password hash
    let current_hash = sqlx::query_scalar::<_, String>(
        "SELECT password_hash FROM users WHERE id = $1 AND is_active = true"
    )
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let current_hash = current_hash.ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    // Verify current password
    let password_valid = PasswordService::verify_password(&request.current_password, &current_hash)?;
    if !password_valid {
        return Err(ApiError::BadRequest("Current password is incorrect".to_string()));
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

/// Get user by ID (admin only)
#[utoipa::path(
    get,
    path = "/api/users/{id}",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User found", body = UserInfo),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserInfo>> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, blockchain_registered,
                is_active, email_verified, created_at, updated_at
         FROM users
         WHERE id = $1"
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let user_data = user.ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let user_info = UserInfo {
        id: user_data.id,
        username: user_data.username,
        email: user_data.email,
        role: user_data.role,
        wallet_address: user_data.wallet_address,
    };

    Ok(Json(user_info))
}

/// List users with search and pagination (admin only)
#[utoipa::path(
    get,
    path = "/api/users",
    tag = "users",
    params(
        UserSearchQuery
    ),
    responses(
        (status = 200, description = "Users list retrieved successfully", body = UserListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin/Faculty access required")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    Query(mut params): Query<UserSearchQuery>,
) -> Result<Json<UserListResponse>> {
    // Validate and normalize parameters
    params.validate()?;
    
    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build WHERE clause
    let mut where_conditions = vec!["is_active = true".to_string()];
    let mut bind_values: Vec<Box<dyn sqlx::Encode<'_, sqlx::Postgres> + Send>> = Vec::new();
    let mut param_count = 1;

    if let Some(search) = &params.search {
        where_conditions.push(format!(
            "(username ILIKE ${} OR email ILIKE ${} OR first_name ILIKE ${} OR last_name ILIKE ${})",
            param_count, param_count + 1, param_count + 2, param_count + 3
        ));
        let search_pattern = format!("%{}%", search);
        bind_values.push(Box::new(search_pattern.clone()));
        bind_values.push(Box::new(search_pattern.clone()));
        bind_values.push(Box::new(search_pattern.clone()));
        bind_values.push(Box::new(search_pattern));
        param_count += 4;
    }

    if let Some(role) = &params.role {
        where_conditions.push(format!("role = ${}", param_count));
        bind_values.push(Box::new(role.clone()));
        param_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Get total count
    let count_query = format!("SELECT COUNT(*) FROM users WHERE {}", where_clause);
    let total = sqlx::query_scalar::<_, i64>(&count_query)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Get users with dynamic sorting
    let users_query = format!(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, blockchain_registered,
                is_active, email_verified, created_at, updated_at
         FROM users 
         WHERE {} 
         ORDER BY {} {}
         LIMIT ${} OFFSET ${}",
        where_clause, sort_field, sort_direction, param_count, param_count + 1
    );

    let users_data = sqlx::query_as::<_, UserRow>(&users_query)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let users: Vec<UserInfo> = users_data
        .into_iter()
        .map(|user| UserInfo {
            id: user.id,
            username: user.username,
            email: user.email,
            role: user.role,
            wallet_address: user.wallet_address,
        })
        .collect();

    // Create pagination metadata
    let pagination = crate::utils::PaginationMeta::new(
        &crate::utils::PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    let response = UserListResponse {
        data: users,
        pagination,
    };

    Ok(Json(response))
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    role: String,
    #[allow(dead_code)]
    first_name: String,
    #[allow(dead_code)]
    last_name: String,
    wallet_address: Option<String>,
    #[allow(dead_code)]
    blockchain_registered: bool,
    #[allow(dead_code)]
    is_active: bool,
    email_verified: bool,
    #[allow(dead_code)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[allow(dead_code)]
    updated_at: chrono::DateTime<chrono::Utc>,
}
