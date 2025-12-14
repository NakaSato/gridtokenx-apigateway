use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Current epoch response
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentEpochResponse {
    pub id: String,
    pub epoch_number: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub clearing_price: Option<String>,
    pub total_volume: String,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub time_remaining_seconds: i64,
}

/// Epoch history query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct EpochHistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
}

pub fn default_limit() -> i64 {
    20
}

/// Epoch history item
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochHistoryItem {
    pub id: String,
    pub epoch_number: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub clearing_price: Option<String>,
    pub total_volume: String,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Epoch history response
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochHistoryResponse {
    pub epochs: Vec<EpochHistoryItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Epoch statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochStatsResponse {
    pub id: String,
    pub epoch_number: i64,
    pub status: String,
    pub duration_minutes: i64,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub match_rate_percent: f64,
    pub total_volume: String,
    pub clearing_price: Option<String>,
    pub unique_traders: i64,
    pub settlements_pending: i64,
    pub settlements_confirmed: i64,
}

/// Manual clearing request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ManualClearingRequest {
    pub reason: Option<String>,
}

/// Manual clearing response
#[derive(Debug, Serialize, ToSchema)]
pub struct ManualClearingResponse {
    pub success: bool,
    pub message: String,
    pub epoch_id: String,
    pub triggered_at: DateTime<Utc>,
}
