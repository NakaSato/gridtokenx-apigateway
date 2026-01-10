//! Meter Management Module
//!
//! This module provides API endpoints for meter management including:
//! - Submitting meter readings
//! - Retrieving reading history
//! - Token minting from readings
//! - Meter registration and verification

pub mod minting;
pub mod stub;
pub mod types;
pub mod zones;

// Re-export from stub module
pub use stub::{
    submit_reading, meter_health,
    MeterReadingResponse,
    __path_get_meter_readings, __path_get_meter_trends, __path_get_meter_health,
};

// Re-export minting handlers
pub use minting::{mint_from_reading, mint_user_reading};

// Re-export types
pub use types::{MintFromReadingRequest, MintResponse, SubmitReadingRequest, ReadingData};

// Re-export zone handlers
pub use zones::{
    get_zones, get_zone_stats, ZoneSummary, ZoneStats,
    __path_get_zones, __path_get_zone_stats,
};

