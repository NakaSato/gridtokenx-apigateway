use axum::{extract::State, response::Json, Extension};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::Claims;
use crate::error::ApiError;
use crate::services::{KeyRotationService, RotationReport, RotationStatus};
use crate::AppState;

/// Request to initiate key rotation
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct InitiateRotationRequest {
    /// New encryption secret (must be at least 32 characters)
    #[validate(length(min = 32, max = 128))]
    pub new_secret: String,

    /// New key version number
    pub new_version: i32,

    /// Confirmation flag (must be true to proceed)
    pub confirm: bool,
}

/// Response from key rotation
#[derive(Debug, Serialize, ToSchema)]
pub struct RotationResponse {
    pub success: bool,
    pub report: RotationReport,
}

/// Request to rollback key rotation
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RollbackRequest {
    /// Target version to rollback to
    pub target_version: i32,

    /// Current encryption secret
    #[validate(length(min = 32, max = 128))]
    pub current_secret: String,

    /// Target encryption secret (the old key)
    #[validate(length(min = 32, max = 128))]
    pub target_secret: String,

    /// Confirmation flag
    pub confirm: bool,
}

/// Initiate encryption key rotation
///
/// This is a high-risk operation that re-encrypts all user wallets with a new encryption key.
/// Only accessible by administrators.
#[utoipa::path(
    post,
    path = "/api/admin/keys/rotate",
    tag = "Admin - Key Rotation",
    request_body = InitiateRotationRequest,
    responses(
        (status = 200, description = "Key rotation completed", body = RotationResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Rotation failed")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn initiate_rotation_handler(
    State(state): State<AppState>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<InitiateRotationRequest>,
) -> crate::error::Result<Json<RotationResponse>> {
    // Verify admin role
    if user.role != "admin" {
        return Err(ApiError::Forbidden(
            "Only administrators can rotate encryption keys".to_string(),
        ));
    }

    // Validate confirmation
    if !payload.confirm {
        return Err(ApiError::BadRequest(
            "Confirmation required to proceed with key rotation".to_string(),
        ));
    }

    // Validate new version is higher than current
    let status = KeyRotationService::new(state.db.clone())
        .get_rotation_status()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get rotation status: {}", e)))?;

    if payload.new_version <= status.current_version {
        return Err(ApiError::BadRequest(format!(
            "New version ({}) must be greater than current version ({})",
            payload.new_version, status.current_version
        )));
    }

    tracing::warn!(
        "Admin {} initiating key rotation to version {}",
        user.sub,
        payload.new_version
    );

    // Perform rotation
    let rotation_service = KeyRotationService::new(state.db.clone());
    let report = rotation_service
        .rotate_all_keys(
            &state.config.encryption_secret,
            &payload.new_secret,
            payload.new_version,
        )
        .await
        .map_err(|e| {
            tracing::error!("Key rotation failed: {}", e);
            ApiError::Internal(format!("Key rotation failed: {}", e))
        })?;

    tracing::info!(
        "Key rotation completed: {}/{} successful",
        report.successful,
        report.total_users
    );

    Ok(Json(RotationResponse {
        success: report.failed == 0,
        report,
    }))
}

/// Get current key rotation status
#[utoipa::path(
    get,
    path = "/api/admin/keys/status",
    tag = "Admin - Key Rotation",
    responses(
        (status = 200, description = "Current rotation status", body = RotationStatus),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Failed to get status")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_rotation_status_handler(
    State(state): State<AppState>,
    Extension(user): Extension<Claims>,
) -> crate::error::Result<Json<RotationStatus>> {
    // Verify admin role
    if user.role != "admin" {
        return Err(ApiError::Forbidden(
            "Only administrators can view key rotation status".to_string(),
        ));
    }

    let rotation_service = KeyRotationService::new(state.db.clone());
    let status = rotation_service
        .get_rotation_status()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get rotation status: {}", e)))?;

    Ok(Json(status))
}

/// Rollback to a previous key version
#[utoipa::path(
    post,
    path = "/api/admin/keys/rollback",
    tag = "Admin - Key Rotation",
    request_body = RollbackRequest,
    responses(
        (status = 200, description = "Rollback completed", body = RotationResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Rollback failed")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn rollback_rotation_handler(
    State(state): State<AppState>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<RollbackRequest>,
) -> crate::error::Result<Json<RotationResponse>> {
    // Verify admin role
    if user.role != "admin" {
        return Err(ApiError::Forbidden(
            "Only administrators can rollback key rotation".to_string(),
        ));
    }

    // Validate confirmation
    if !payload.confirm {
        return Err(ApiError::BadRequest(
            "Confirmation required to proceed with rollback".to_string(),
        ));
    }

    tracing::warn!(
        "Admin {} initiating rollback to version {}",
        user.sub,
        payload.target_version
    );

    // Perform rollback
    let rotation_service = KeyRotationService::new(state.db.clone());
    let report = rotation_service
        .rollback_rotation(
            payload.target_version,
            &payload.current_secret,
            &payload.target_secret,
        )
        .await
        .map_err(|e| {
            tracing::error!("Rollback failed: {}", e);
            ApiError::Internal(format!("Rollback failed: {}", e))
        })?;

    tracing::info!(
        "Rollback completed: {}/{} successful",
        report.successful,
        report.total_users
    );

    Ok(Json(RotationResponse {
        success: report.failed == 0,
        report,
    }))
}
