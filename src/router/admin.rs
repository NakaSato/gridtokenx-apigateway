//! Admin-only routes (placeholder for future expansion).
//!
//! Most admin routes are currently in protected.rs.
//! This module can be used to separate admin routes further if needed.

use axum::Router;
use crate::app_state::AppState;

/// Build admin-only routes.
///
/// Currently, admin routes are integrated into protected_routes.
/// This function is provided for future expansion when admin routes
/// need to be separated into their own module with different middleware.
pub fn admin_routes() -> Router<AppState> {
    Router::new()
    // Future: Add admin-specific routes here with different middleware
    // e.g., IP whitelist, additional audit logging, etc.
}
