//! User activity tracking handlers

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::handlers::authorization::require_admin_or_owner;
use crate::AppState;

use super::types::{ActivityListResponse, ActivityQuery, ActivityRow, UserActivity};

/// Get current user's activity log
#[utoipa::path(
    get,
    path = "/api/user/activity",
    tag = "users",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "User activity retrieved successfully", body = ActivityListResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_my_activity(
    State(state): State<AppState>,
    Query(params): Query<ActivityQuery>,
    user: AuthenticatedUser,
) -> Result<Json<ActivityListResponse>> {
    // Use the current user's ID from the JWT token
    let user_id = user.0.sub;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * per_page;

    // Get activities - using correct column names from migration
    let activities = sqlx::query_as::<_, ActivityRow>(
        "SELECT id, user_id, activity_type, description, ip_address, user_agent, created_at
         FROM user_activities
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(per_page as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Get total count
    let total =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_activities WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let activity_list: Vec<UserActivity> = activities
        .into_iter()
        .map(|row| UserActivity {
            id: row.id,
            user_id: row.user_id,
            action: row.activity_type, // Map activity_type to action
            details: row.description,  // Map description to details
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
        (status = 200, description = "User activity retrieved successfully", body = ActivityListResponse),
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
    require_admin_or_owner(&user.0, user_id)?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * per_page;

    // Get activities - using correct column names from migration
    let activities = sqlx::query_as::<_, ActivityRow>(
        "SELECT id, user_id, activity_type, description, ip_address, user_agent, created_at
         FROM user_activities
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(per_page as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Get total count
    let total =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_activities WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let activity_list: Vec<UserActivity> = activities
        .into_iter()
        .map(|row| UserActivity {
            id: row.id,
            user_id: row.user_id,
            action: row.activity_type,
            details: row.description,
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
