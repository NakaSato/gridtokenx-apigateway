use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Helper trait to unify reading data for analysis
pub trait ReadingData {
    fn voltage(&self) -> Option<f64>;
    fn frequency(&self) -> Option<f64>;
    fn battery_level(&self) -> Option<f64>;
    fn power_factor(&self) -> Option<f64>;
    fn thd_voltage(&self) -> Option<f64>;
    fn thd_current(&self) -> Option<f64>;
}

/// Request to submit a meter reading (Simulator/Stub)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SubmitReadingRequest {
    pub wallet_address: Option<String>,
    #[schema(value_type = f64)]
    pub kwh_amount: Decimal,
    pub reading_timestamp: DateTime<Utc>,
    pub meter_signature: Option<String>,
    pub meter_serial: Option<String>,
    pub meter_id: Option<Uuid>,

    // Energy Data (kWh)
    pub energy_generated: Option<f64>,
    pub energy_consumed: Option<f64>,
    pub surplus_energy: Option<f64>,
    pub deficit_energy: Option<f64>,

    // Electrical Parameters
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub power_factor: Option<f64>,
    pub frequency: Option<f64>,
    pub temperature: Option<f64>,
    pub thd_voltage: Option<f64>,
    pub thd_current: Option<f64>,

    // GPS Location
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub zone_id: Option<i32>,

    // Battery & Environmental
    pub battery_level: Option<f64>,
}

impl ReadingData for SubmitReadingRequest {
    fn voltage(&self) -> Option<f64> { self.voltage }
    fn frequency(&self) -> Option<f64> { self.frequency }
    fn battery_level(&self) -> Option<f64> { self.battery_level }
    fn power_factor(&self) -> Option<f64> { self.power_factor }
    fn thd_voltage(&self) -> Option<f64> { self.thd_voltage }
    fn thd_current(&self) -> Option<f64> { self.thd_current }
}

/// Request to mint tokens from a reading (admin only)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct MintFromReadingRequest {
    /// The reading ID (UUID) to mint tokens from
    pub reading_id: Uuid,
}

/// Response after minting tokens
#[derive(Debug, Serialize, ToSchema)]
pub struct MintResponse {
    /// Success message
    pub message: String,
    /// Transaction signature on Solana
    pub transaction_signature: String,
    /// Amount of kWh minted
    #[schema(value_type = f64)]
    pub kwh_amount: Decimal,
    /// Wallet address that received tokens
    pub wallet_address: String,
}

