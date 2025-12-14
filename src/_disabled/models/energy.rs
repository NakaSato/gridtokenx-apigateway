use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EnergyReading {
    pub id: Option<Uuid>,
    pub meter_id: String,
    pub timestamp: DateTime<Utc>,
    pub energy_generated: f64,
    pub energy_consumed: f64,
    pub solar_irradiance: Option<f64>,
    pub temperature: Option<f64>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// Internal database model with BigDecimal for database operations
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnergyReadingDb {
    pub id: Option<Uuid>,
    pub meter_id: String,
    pub timestamp: DateTime<Utc>,
    pub energy_generated: Decimal,
    pub energy_consumed: Decimal,
    pub solar_irradiance: Option<Decimal>,
    pub temperature: Option<Decimal>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>, // Make this optional to handle defaults
}

impl From<EnergyReadingDb> for EnergyReading {
    fn from(db_reading: EnergyReadingDb) -> Self {
        EnergyReading {
            id: db_reading.id,
            meter_id: db_reading.meter_id,
            timestamp: db_reading.timestamp,
            energy_generated: db_reading.energy_generated.to_f64().unwrap_or(0.0),
            energy_consumed: db_reading.energy_consumed.to_f64().unwrap_or(0.0),
            solar_irradiance: db_reading.solar_irradiance.and_then(|d| d.to_f64()),
            temperature: db_reading.temperature.and_then(|d| d.to_f64()),
            metadata: db_reading.metadata,
            created_at: db_reading.created_at.unwrap_or_else(|| Utc::now()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EnergyReadingSubmission {
    pub meter_id: String,
    pub timestamp: DateTime<Utc>,
    pub energy_generated: f64,
    pub energy_consumed: f64,
    pub solar_irradiance: Option<f64>,
    pub temperature: Option<f64>,
    pub engineering_authority_signature: String,
    pub metadata: Option<EnergyMetadata>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EnergyMetadata {
    pub location: String,
    pub device_type: String,
    pub weather_conditions: Option<String>,
}
