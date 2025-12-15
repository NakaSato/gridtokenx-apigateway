//! Status Handlers Module
//!
//! System and service status endpoint handlers.

use axum::Json;

use super::types::StatusResponse;

/// Get system status
/// GET /api/v1/status
pub async fn system_status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok".to_string(),
        version: "1.0.0".to_string(),
        uptime: "running".to_string(),
    })
}

/// Get meter service status
/// GET /api/v1/status/meters
pub async fn meter_status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok".to_string(),
        version: "1.0.0".to_string(),
        uptime: "meter service running".to_string(),
    })
}
