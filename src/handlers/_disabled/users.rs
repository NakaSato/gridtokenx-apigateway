use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::UserInfo;
use crate::error::{ApiError, Result};
use crate::AppState;

use super::UserRow;

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
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid sort_by field: {}. Allowed: created_at, username, email, role",
                        sort_by
                    )));
                }
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
         WHERE id = $1",
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
        where_clause,
        sort_field,
        sort_direction,
        param_count,
        param_count + 1
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
