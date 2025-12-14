//! Handlers module - Reorganized structure
//!
//! Provides API handlers organized by domain:
//! - `auth/` - Authentication handlers (login, register, profile)
//! - `meter/` - Meter management handlers (readings, registration)
//! - `blockchain/` - Blockchain interaction handlers
//! - `websocket/` - WebSocket handlers
//! - `common/` - Shared utilities (extractors, response types)
//! - `_disabled/` - Disabled/legacy handlers (not exported)

// Domain handlers
pub mod auth;
pub mod blockchain;
pub mod meter;
pub mod websocket;

// Shared utilities
pub mod common;

// Re-export commonly used types from common
pub use common::{
    DateRangeParams, PaginationParams, SearchParams, SortOrder, ValidatedUuid,
    ApiResponse, ListResponse, PaginatedResponse,
};

// Re-export V1 route builders (new RESTful API)
pub use auth::{v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes};

// Re-export legacy route builders (backward compatibility)
pub use auth::{auth_routes, token_routes, user_meter_routes, meter_info_routes};

// Re-export meter routes
pub use meter::meter_routes;
