//! Governance API Module
//!
//! Provides endpoints for:
//! - Checking governance system status
//! - Emergency pause/unpause actions (admin only)

pub mod actions;
pub mod status;
pub mod types;
pub mod utils;

pub use actions::*;
pub use status::*;
pub use types::*;
pub use utils::*;
