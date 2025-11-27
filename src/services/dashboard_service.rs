use crate::services::event_processor_service::{EventProcessorService, EventProcessorStats};
use crate::services::health_check::{DetailedHealthStatus, HealthChecker};
use crate::services::transaction_metrics::MetricsExporter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMetrics {
    pub system_health: DetailedHealthStatus,
    pub event_processor: EventProcessorStats,
    pub pending_transactions: HashMap<String, i64>,
}

#[derive(Clone)]
pub struct DashboardService {
    health_checker: HealthChecker,
    event_processor: EventProcessorService,
}

impl DashboardService {
    pub fn new(health_checker: HealthChecker, event_processor: EventProcessorService) -> Self {
        Self {
            health_checker,
            event_processor,
        }
    }

    pub async fn get_metrics(&self) -> anyhow::Result<DashboardMetrics> {
        // Fetch metrics in parallel where possible
        let (health_status, event_stats) = tokio::join!(
            self.health_checker.perform_health_check(),
            self.event_processor.get_stats()
        );

        let pending_transactions = MetricsExporter::get_transaction_stats();

        Ok(DashboardMetrics {
            system_health: health_status,
            event_processor: event_stats?,
            pending_transactions,
        })
    }
}
