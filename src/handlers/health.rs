use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::EventProcessorStats;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub environment: String,
    pub dependencies: Vec<ServiceHealth>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServiceHealth {
    pub name: String,
    pub status: String,
    pub response_time_ms: Option<u64>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub error_message: Option<String>,
}

impl HealthStatus {
    pub fn new() -> Self {
        Self {
            status: "healthy".to_string(),
            timestamp: chrono::Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            dependencies: Vec::new(),
        }
    }

    pub fn add_dependency_check(
        &mut self,
        name: &str,
        is_healthy: bool,
        response_time: Option<u64>,
        error: Option<String>,
    ) {
        self.dependencies.push(ServiceHealth {
            name: name.to_string(),
            status: if is_healthy {
                "healthy".to_string()
            } else {
                "unhealthy".to_string()
            },
            response_time_ms: response_time,
            last_check: chrono::Utc::now(),
            error_message: error,
        });

        // Update overall status if any dependency is unhealthy
        if !is_healthy {
            self.status = "degraded".to_string();
        }
    }
}

/// Basic health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthStatus)
    )
)]
pub async fn health_check() -> Json<HealthStatus> {
    Json(HealthStatus::new())
}

/// Get event processor statistics
/// GET /api/health/event-processor
#[utoipa::path(
    get,
    path = "/api/health/event-processor",
    tag = "health",
    responses(
        (status = 200, description = "Event processor statistics", body = EventProcessorStats),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_event_processor_stats(
    State(state): State<crate::AppState>,
) -> Result<Json<crate::services::EventProcessorStats>, crate::error::ApiError> {
    let stats = state
        .event_processor_service
        .get_stats()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get event processor stats: {}", e);
            crate::error::ApiError::Internal("Failed to get event processor stats".to_string())
        })?;

    Ok(Json(stats))
}
