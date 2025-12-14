use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

use crate::database::schema::types::EpochStatus;
use crate::services::market_clearing::MarketEpoch;

pub fn calculate_epoch_number(timestamp: DateTime<Utc>) -> i64 {
    (timestamp.year() as i64) * 100_000_000
        + (timestamp.month() as i64) * 1_000_000
        + (timestamp.day() as i64) * 10_000
        + (timestamp.hour() as i64) * 100
        + ((timestamp.minute() / 15) * 15) as i64
}

pub fn calculate_next_epoch_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let current_epoch_start = now
        .with_minute((now.minute() / 15) * 15)
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .unwrap_or(now);

    current_epoch_start + Duration::minutes(15)
}

pub fn determine_target_state(epoch: &MarketEpoch, now: DateTime<Utc>) -> String {
    if now < epoch.start_time {
        "pending".to_string()
    } else if now >= epoch.start_time && now < epoch.end_time {
        "active".to_string()
    } else if now >= epoch.end_time && epoch.status != EpochStatus::Settled {
        "cleared".to_string()
    } else {
        epoch.status.to_string()
    }
}
