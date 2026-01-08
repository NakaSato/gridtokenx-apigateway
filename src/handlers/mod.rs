//! Handlers module - Reorganized structure
//!
//! Provides API handlers organized by domain:
//! - `auth/` - Authentication handlers (login, register, profile)
//! - `meter/` - Meter management handlers (readings, registration)
//! - `blockchain/` - Blockchain interaction handlers
//! - `websocket/` - WebSocket handlers
//! - `notifications` - Push notification handlers
//! - `common/` - Shared utilities (extractors, response types)
//! - `_disabled/` - Disabled/legacy handlers (not exported)

// Domain handlers
pub mod auth;
pub mod blockchain;
pub mod meter;
pub mod dev;
pub mod trading;
pub mod futures;
pub mod dashboard;
pub mod analytics;
pub mod websocket;
pub mod rpc;
pub mod proxy;
pub mod notifications;

// Shared utilities
pub mod common;

// Re-export commonly used types from common
pub use common::{
    DateRangeParams, PaginationParams, SearchParams, SortOrder, ValidatedUuid,
    ApiResponse, ListResponse, PaginatedResponse,
};

// Re-export V1 route builders (new RESTful API)
pub use auth::{v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes};
pub use trading::v1_trading_routes;
pub use dashboard::v1_dashboard_routes;
