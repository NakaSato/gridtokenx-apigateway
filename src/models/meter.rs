use serde::{Deserialize, Serialize};

/// Status of a meter reading
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MeterReadingStatus {
    /// Reading is from a legacy meter before verification system
    LegacyUnverified,
    /// Reading is pending verification
    Pending,
    /// Reading has been verified and is valid
    Verified,
    /// Reading has been rejected
    Rejected,
}

impl Default for MeterReadingStatus {
    fn default() -> Self {
        Self::LegacyUnverified
    }
}

impl std::fmt::Display for MeterReadingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeterReadingStatus::LegacyUnverified => write!(f, "legacy_unverified"),
            MeterReadingStatus::Pending => write!(f, "pending"),
            MeterReadingStatus::Verified => write!(f, "verified"),
            MeterReadingStatus::Rejected => write!(f, "rejected"),
        }
    }
}

impl std::str::FromStr for MeterReadingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "legacy_unverified" => Ok(MeterReadingStatus::LegacyUnverified),
            "pending" => Ok(MeterReadingStatus::Pending),
            "verified" => Ok(MeterReadingStatus::Verified),
            "rejected" => Ok(MeterReadingStatus::Rejected),
            _ => Err(format!("Invalid meter reading status: {}", s)),
        }
    }
}

// Re-export meter-related models from services
pub use crate::services::meter_service::MeterReading;
