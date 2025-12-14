//! Meter Management Module
//!
//! This module provides API endpoints for meter management including:
//! - Submitting meter readings
//! - Retrieving reading history
//! - Token minting from readings
//! - Meter registration and verification

pub mod stub;

// Re-export from stub module
pub use stub::{
    meter_routes, submit_reading, meter_health,
    SubmitReadingRequest, MeterReadingResponse,
};
