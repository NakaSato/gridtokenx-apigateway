use axum::{
    extract::{Path, State},
    response::Json,
};
use tracing::{error, info};
use uuid::Uuid;

use super::types::UpdateUserRoleRequest;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Update user role/status (admin only)
/// POST /api/admin/users/:id/update-role
#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/update-role",
    tag = "registry",
    request_body = UpdateUserRoleRequest,
    security(("bearer_auth" = [])),
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User role update initiated"),
        (status = 400, description = "Invalid request or user has no wallet"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_user_role(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserRoleRequest>,
) -> Result<Json<serde_json::Value>> {
    info!(
        "Admin {} updating user {} role to {:?}",
        user.0.sub, user_id, payload.new_status
    );

    // Check if user is admin (in database)
    let db_user = sqlx::query!(
        "SELECT id, email, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch admin user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("Admin user not found".to_string()))?;

    // Verify admin role
    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can update user roles".to_string(),
        ));
    }

    // Get the target user from database
    let target_user = sqlx::query!(
        "SELECT id, email, wallet_address, role::text as role FROM users WHERE id = $1",
        user_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch target user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("Target user not found".to_string()))?;

    // Get wallet address
    let wallet_address = target_user
        .wallet_address
        .ok_or_else(|| ApiError::BadRequest("User has no wallet address".to_string()))?;

    info!(
        "Would update blockchain status for wallet {} to {:?}",
        wallet_address, payload.new_status
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "User role update initiated",
        "user_id": user_id,
        "wallet_address": wallet_address,
        "new_status": payload.new_status,
        "note": "On-chain transaction not yet implemented - requires transaction signing"
    })))
}
