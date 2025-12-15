//! Routes Module
//!
//! All route builders for V1 RESTful API and legacy routes.

use axum::{
    routing::{get, post},
    Router,
};

use crate::AppState;
use super::{
    login::{login, verify_email},
    registration::{register, resend_verification},
    profile::profile,
    meters::{
        get_my_meters, register_meter, get_registered_meters, 
        get_registered_meters_filtered, update_meter_status, verify_meter, create_reading,
    },
    wallets::token_balance,
    status::{system_status, meter_status},
};

// ============================================================================
// V1 RESTful API Routes (New)
// ============================================================================

/// Build V1 auth routes: POST /api/v1/auth/token, GET /api/v1/auth/verify
pub fn v1_auth_routes() -> Router<AppState> {
    Router::new()
        .route("/token", post(login))  // POST /api/v1/auth/token
        .route("/verify", get(verify_email))  // GET /api/v1/auth/verify
}

/// Build V1 users routes: POST /api/v1/users, GET /api/v1/users/me
pub fn v1_users_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(register))  // POST /api/v1/users (register)
        .route("/me", get(profile))  // GET /api/v1/users/me
        .route("/me/meters", get(get_my_meters))  // GET /api/v1/users/me/meters
}

/// Build V1 meters routes
pub fn v1_meters_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(register_meter))  // POST /api/v1/meters
        .route("/", get(get_registered_meters_filtered))  // GET /api/v1/meters?status=verified
        .route("/{serial}", axum::routing::patch(update_meter_status))  // PATCH /api/v1/meters/{serial}
        .route("/{serial}/readings", post(create_reading))  // POST /api/v1/meters/{serial}/readings
}

/// Build V1 wallets routes
pub fn v1_wallets_routes() -> Router<AppState> {
    Router::new()
        .route("/{address}/balance", get(token_balance))  // GET /api/v1/wallets/{address}/balance
}

/// Build V1 status routes
pub fn v1_status_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(system_status))  // GET /api/v1/status
        .route("/meters", get(meter_status))  // GET /api/v1/status/meters
}

// ============================================================================
// Legacy Routes (Backward Compatibility)
// ============================================================================

/// Build legacy auth routes (deprecated)
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/profile", get(profile))
        .route("/verify-email", get(verify_email))
        .route("/resend-verification", post(resend_verification))
}

/// Build legacy token routes (deprecated)
pub fn token_routes() -> Router<AppState> {
    Router::new()
        .route("/balance/{wallet_address}", get(token_balance))
}

/// Build legacy user meter routes (deprecated)
pub fn user_meter_routes() -> Router<AppState> {
    Router::new()
        .route("/profile", get(profile))
        .route("/meters", get(get_my_meters))
        .route("/meters", post(register_meter))
}

/// Build legacy meter info routes (deprecated)
pub fn meter_info_routes() -> Router<AppState> {
    Router::new()
        .route("/registered", get(get_registered_meters))
        .route("/verify", post(verify_meter))
}
