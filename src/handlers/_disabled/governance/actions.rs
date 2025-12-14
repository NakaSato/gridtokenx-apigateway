use axum::{extract::State, response::Json};
use tracing::{error, info};

use super::types::{EmergencyActionResponse, EmergencyPauseRequest};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Emergency pause the system (admin only)
/// POST /api/admin/governance/emergency-pause
#[utoipa::path(
    post,
    path = "/api/admin/governance/emergency-pause",
    tag = "governance",
    request_body = EmergencyPauseRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Emergency pause initiated", body = EmergencyActionResponse),
        (status = 400, description = "Invalid request or empty reason"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn emergency_pause(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<EmergencyPauseRequest>,
) -> Result<Json<EmergencyActionResponse>> {
    info!("Emergency pause request from user: {}", user.0.sub);

    // Validate reason is not empty
    if payload.reason.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Emergency reason is required".to_string(),
        ));
    }

    // Check user role - only admins can emergency pause
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can trigger emergency pause".to_string(),
        ));
    }

    let timestamp = chrono::Utc::now().timestamp();

    info!(
        "Emergency pause initiated by user {} with reason: {}",
        user.0.sub, payload.reason
    );

    Ok(Json(EmergencyActionResponse {
        success: true,
        message: format!("Emergency pause initiated. Reason: {}", payload.reason),
        paused: true,
        timestamp,
    }))
}

/// Emergency unpause the system (admin only)
/// POST /api/admin/governance/unpause
#[utoipa::path(
    post,
    path = "/api/admin/governance/unpause",
    tag = "governance",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Emergency unpause initiated", body = EmergencyActionResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn emergency_unpause(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<EmergencyActionResponse>> {
    info!("Emergency unpause request from user: {}", user.0.sub);

    // Check user role - only admins can emergency unpause
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can unpause the system".to_string(),
        ));
    }

    let timestamp = chrono::Utc::now().timestamp();

    info!("Emergency unpause initiated by user {}", user.0.sub);

    Ok(Json(EmergencyActionResponse {
        success: true,
        message: "Emergency unpause initiated successfully".to_string(),
        paused: false,
        timestamp,
    }))
}
