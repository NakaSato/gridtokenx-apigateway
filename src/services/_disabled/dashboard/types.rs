use crate::services::event_processor::EventProcessorStats;
use crate::services::health_check::DetailedHealthStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMetrics {
    pub system_health: DetailedHealthStatus,
    pub event_processor: EventProcessorStats,
    pub pending_transactions: HashMap<String, i64>,
}
