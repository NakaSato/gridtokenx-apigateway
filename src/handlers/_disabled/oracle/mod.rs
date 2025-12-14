//! Oracle API Module
//!
//! Provides endpoints for:
//! - Submitting price updates
//! - Retrieving current price data
//! - Checking oracle system status from blockchain

pub mod data;
pub mod prices;
pub mod types;

pub use data::*;
pub use prices::*;
pub use types::*;
