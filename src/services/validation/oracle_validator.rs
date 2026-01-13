use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use tracing::{warn, error};
use uuid::Uuid;
use crate::handlers::auth::types::CreateReadingRequest;

pub struct OracleValidator;

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub max_kwh_per_reading: f64,
    pub allow_negative_generation: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_kwh_per_reading: 1000.0, // Sanity threshold for a 15-min reading
            allow_negative_generation: false,
        }
    }
}

impl OracleValidator {
    /// Validate a meter reading against business rules and physical constraints
    pub async fn validate_reading(
        meter_serial: &str,
        request: &CreateReadingRequest,
        config: &ValidationConfig,
    ) -> Result<()> {
        // 1. Basic Range Check
        if request.kwh > config.max_kwh_per_reading {
            let msg = format!(
                "Reading for {} exceeds sanity threshold: {} kWh > {} kWh",
                meter_serial, request.kwh, config.max_kwh_per_reading
            );
            warn!("🚨 {}", msg);
            return Err(anyhow!(msg));
        }

        // 2. Generation Domain Check
        if !config.allow_negative_generation && request.kwh < 0.0 {
            let msg = format!(
                "Negative energy generation detected for {}: {} kWh",
                meter_serial, request.kwh
            );
            error!("❌ Oracle Violation: {}", msg);
            return Err(anyhow!(msg));
        }

        // 3. Asset Integrity Check
        // If both surplus and deficit are provided, ensure they are logical
        if let (Some(surplus), Some(deficit)) = (request.surplus_energy, request.deficit_energy) {
            if surplus > 0.0 && deficit > 0.0 {
                let msg = format!("Inconsistent telemetry for {}: simultaneous surplus and deficit", meter_serial);
                warn!("⚠️ {}", msg);
                // We might allow this but alert, or reject. For now, let's reject.
                return Err(anyhow!(msg));
            }
        }

        Ok(())
    }

    /// Check for anomalous spikes compared to previous data (Monotonicity check)
    /// In a real system, this would query the DB for the last cumulative reading.
    pub fn check_monotonicity(
        _meter_id: Uuid,
        _current_cumulative: Decimal,
        _previous_cumulative: Decimal,
    ) -> Result<()> {
        if _current_cumulative < _previous_cumulative {
            return Err(anyhow!("Counter reset detected or invalid decrement in cumulative reading"));
        }
        Ok(())
    }
}
