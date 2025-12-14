use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use uuid::Uuid;

use super::types::{AuditLogQuery, AuditLogsResponse};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Get audit logs for a specific user (admin only)
///
/// GET /api/admin/audit/user/{user_id}
#[utoipa::path(
    get,
    path = "/api/admin/audit/user/{user_id}",
    tag = "admin",
    params(
        ("user_id" = Uuid, Path, description = "User ID to get audit logs for"),
        ("limit" = u32, Query, description = "Number of events to return (max 100)")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User audit logs retrieved successfully", body = AuditLogsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_audit_logs(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(target_user_id): Path<Uuid>,
    Query(params): Query<AuditLogQuery>,
) -> Result<Json<AuditLogsResponse>> {
    // Check if user is admin
    if user.0.role != "admin" {
        return Err(ApiError::Forbidden(
            "Admin access required to view audit logs".to_string(),
        ));
    }

    let limit = params.limit.min(100) as i64;

    let events = state
        .audit_logger
        .get_user_events(target_user_id, limit)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch audit logs: {}", e)))?;

    Ok(Json(AuditLogsResponse {
        total: events.len(),
        events,
        page: params.page,
        limit: params.limit.min(100),
    }))
}

/// Get audit logs by event type (admin only)
///
/// GET /api/admin/audit/type/{event_type}
#[utoipa::path(
    get,
    path = "/api/admin/audit/type/{event_type}",
    tag = "admin",
    params(
        ("event_type" = String, Path, description = "Event type to filter by"),
        ("limit" = u32, Query, description = "Number of events to return (max 100)")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Audit logs retrieved successfully", body = AuditLogsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_audit_logs_by_type(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(event_type): Path<String>,
    Query(params): Query<AuditLogQuery>,
) -> Result<Json<AuditLogsResponse>> {
    // Check if user is admin
    if user.0.role != "admin" {
        return Err(ApiError::Forbidden(
            "Admin access required to view audit logs".to_string(),
        ));
    }

    let limit = params.limit.min(100) as i64;

    let events = state
        .audit_logger
        .get_events_by_type(&event_type, limit)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch audit logs: {}", e)))?;

    Ok(Json(AuditLogsResponse {
        total: events.len(),
        events,
        page: params.page,
        limit: params.limit.min(100),
    }))
}

/// Get recent security events (admin only)
///
/// GET /api/admin/audit/security
#[utoipa::path(
    get,
    path = "/api/admin/audit/security",
    tag = "admin",
    params(
        ("limit" = u32, Query, description = "Number of events to return (max 100)")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Security events retrieved successfully", body = AuditLogsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_security_events(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<AuditLogQuery>,
) -> Result<Json<AuditLogsResponse>> {
    // Check if user is admin
    if user.0.role != "admin" {
        return Err(ApiError::Forbidden(
            "Admin access required to view security events".to_string(),
        ));
    }

    let limit = params.limit.min(100) as i64;

    let events = state
        .audit_logger
        .get_security_events(limit)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch security events: {}", e)))?;

    Ok(Json(AuditLogsResponse {
        total: events.len(),
        events,
        page: params.page,
        limit: params.limit.min(100),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_bounds() {
        // Simple test to ensure limit logic is reusable if extracted, though here it's embedded in handlers
        let limit = 200_u32;
        let bounded = limit.min(100);
        assert_eq!(bounded, 100);
    }
}
