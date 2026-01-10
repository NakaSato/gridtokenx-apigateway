use crate::services::event_processor::EventProcessorStats;
use crate::services::health_check::DetailedHealthStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct GridStatus {
    pub total_generation: f64,
    pub total_consumption: f64,
    pub net_balance: f64,
    pub active_meters: i64,
    pub co2_saved_kg: f64,
    #[sqlx(skip)]
    pub zones: HashMap<i32, ZoneGridStatus>,
    #[serde(skip)]
    #[sqlx(default)]
    pub zones_data: Option<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ZoneGridStatus {
    pub zone_id: i32,
    pub generation: f64,
    pub consumption: f64,
    pub net_balance: f64,
    pub active_meters: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMetrics {
    pub system_health: DetailedHealthStatus,
    pub event_processor: EventProcessorStats,
    pub pending_transactions: HashMap<String, i64>,
    pub grid_status: GridStatus,
}
