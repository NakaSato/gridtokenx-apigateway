//! Registry API Module
//!
//! Provides endpoints for:
//! - Fetching user and meter data from the registry smart contract
//! - Managing user roles and statuses

pub mod admin;
pub mod types;
pub mod users;
pub mod utils;

pub use admin::*;
pub use types::*;
pub use users::*;
pub use utils::*;
