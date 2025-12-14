//! User Management Module
//!
//! This module provides API endpoints for user account management including:
//! - User registration with email verification
//! - Wallet address management
//! - Admin user management (update, activate, deactivate)
//! - User activity logging and retrieval
//!
//! # Authentication
//! All endpoints except registration require JWT authentication.
//! Admin endpoints require the "admin" role.

pub mod activity;
pub mod admin;
pub mod registration;
pub mod types;
pub mod wallet;

// Re-export all public items
pub use activity::*;
pub use admin::*;
pub use registration::*;
pub use types::*;
pub use wallet::*;
