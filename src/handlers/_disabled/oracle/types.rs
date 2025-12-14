use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to submit a price update (for future price oracle functionality)
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitPriceRequest {
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub timestamp: Option<i64>,
}

/// Response for price submission
#[derive(Debug, Serialize, ToSchema)]
pub struct PriceSubmissionResponse {
    pub success: bool,
    pub message: String,
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub timestamp: i64,
}

/// Current price data
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentPriceData {
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub last_updated: i64,
    pub source: String,
}

/// Oracle data from blockchain
#[derive(Debug, Serialize, ToSchema)]
pub struct OracleDataResponse {
    pub authority: String,
    pub api_gateway: String,
    pub total_readings: u64,
    pub last_reading_timestamp: i64,
    pub last_clearing: i64,
    pub active: bool,
    pub created_at: i64,
}
