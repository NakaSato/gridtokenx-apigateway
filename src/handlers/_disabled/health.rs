//! Health check handlers for service monitoring.
//!
//! This module provides endpoints for:
//! - Basic health checks
//! - Dependency status monitoring
//! - Event processor statistics

use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{ApiError, Result};
use crate::services::EventProcessorStats;
use crate::AppState;

/// Health status values
pub mod status {
    pub const HEALTHY: &str = "healthy";
    pub const UNHEALTHY: &str = "unhealthy";
    pub const DEGRADED: &str = "degraded";
}

/// Basic health response (for simple health checks)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

/// Comprehensive health status with dependency checks
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub environment: String,
    pub dependencies: Vec<ServiceHealth>,
}

/// Individual service health information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServiceHealth {
    pub name: String,
    pub status: String,
    pub response_time_ms: Option<u64>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthStatus {
    /// Create a new healthy status
    pub fn new() -> Self {
        Self {
            status: status::HEALTHY.to_string(),
            timestamp: chrono::Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            dependencies: Vec::new(),
        }
    }

    /// Add a dependency health check result
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
                status::HEALTHY.to_string()
            } else {
                status::UNHEALTHY.to_string()
            },
            response_time_ms: response_time,
            last_check: chrono::Utc::now(),
            error_message: error,
        });

        // Update overall status if any dependency is unhealthy
        if !is_healthy {
            self.status = status::DEGRADED.to_string();
        }
    }

    /// Check if the overall status is healthy
    pub fn is_healthy(&self) -> bool {
        self.status == status::HEALTHY
    }
}

/// Basic health check endpoint
///
/// Returns a simple health status indicating if the service is running.
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
///
/// Returns detailed statistics about the event processor service.
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
    State(state): State<AppState>,
) -> Result<Json<EventProcessorStats>> {
    let stats = state
        .event_processor
        .get_stats()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get event processor stats: {}", e);
            ApiError::Internal("Failed to get event processor stats".to_string())
        })?;

    Ok(Json(stats))
}
