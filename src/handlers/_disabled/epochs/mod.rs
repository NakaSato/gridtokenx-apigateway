//! Market Epochs API Module
//!
//! Provides endpoints for:
//! - Current epoch status
//! - Epoch history
//! - Admin functions (clearing, stats)

pub mod admin;
pub mod queries;
pub mod types;

pub use admin::*;
pub use queries::*;
pub use types::*;
