use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct EpochConfig {
    pub epoch_duration_minutes: u64,
    pub transition_check_interval_secs: u64,
    pub max_orders_per_epoch: usize,
    pub platform_fee_rate: Decimal,
}

impl Default for EpochConfig {
    fn default() -> Self {
        Self {
            epoch_duration_minutes: 15,
            transition_check_interval_secs: 60,
            max_orders_per_epoch: 10_000,
            platform_fee_rate: Decimal::from_str("0.01").unwrap(),
        }
    }
}

#[derive(Debug, Clone, ToSchema)]
pub struct EpochTransitionEvent {
    pub epoch_id: Uuid,
    pub epoch_number: i64,
    pub old_status: String,
    pub new_status: String,
    pub transition_time: DateTime<Utc>,
}
