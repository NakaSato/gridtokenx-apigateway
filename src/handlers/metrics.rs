// Metrics endpoint for Prometheus
// Provides a Prometheus-compatible metrics endpoint

use crate::{AppState, error::ApiError, services::transaction_metrics::MetricsExporter};
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// Prometheus metrics endpoint
///
/// # Returns
///
/// Returns Prometheus-formatted metrics for monitoring
///
/// # Errors
///
/// Returns an error if metrics collection fails
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "metrics",
    summary = "Prometheus metrics",
    description = "Export Prometheus metrics for monitoring and alerting",
    responses(
        (status = 200, description = "Prometheus metrics", content_type = "text/plain"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("none" = [])
    )
)]
pub async fn get_prometheus_metrics(
    State(_app_state): State<AppState>,
) -> Result<Response, ApiError> {
    // Get metrics in Prometheus format
    let metrics_text = MetricsExporter::get_metrics();

    // Create response with appropriate content type
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
        .body(metrics_text.into())
        .map_err(|_| ApiError::Internal("Failed to create response".to_string()))?)
}

/// Health check endpoint with metrics
///
/// Includes basic metrics in the health check response
#[utoipa::path(
    get,
    path = "/health/metrics",
    tag = "metrics",
    summary = "Health check with metrics",
    description = "Health check endpoint that includes basic metrics",
    responses(
        (status = 200, description = "Health status with metrics", body = serde_json::Value),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("none" = [])
    )
)]
pub async fn get_health_with_metrics(
    State(app_state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    use serde_json::json;

    // Get database connection pool metrics
    let pool_size = app_state.db.size();
    let pool_idle = app_state.db.num_idle();
    let pool_active = pool_size.saturating_sub(pool_idle as u32);

    // For now, use simplified metrics
    let pending_tx_count = 0;
    let confirmed_tx_count = 0;
    let failed_tx_count = 0;

    // TODO: Extract from metrics when metrics exporter is properly implemented
    // let metrics_text = MetricsExporter::get_metrics();
    // Parse metrics_text to extract counts...

    let response = json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "uptime_seconds": 0, // TODO: Add start_time to AppState
        "metrics": {
            "transactions": {
                "pending_count": pending_tx_count,
                "confirmed_count": confirmed_tx_count,
                "failed_count": failed_tx_count,
                "total_count": pending_tx_count + confirmed_tx_count + failed_tx_count
            },
            "database": {
                "pool_size": pool_size,
                "active_connections": pool_active,
                "idle_connections": pool_idle
            }
        }
    });

    Ok(axum::Json(response))
}
