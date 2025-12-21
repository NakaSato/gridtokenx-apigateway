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
    registration::register,
    password_reset::{forgot_password, reset_password, change_password},
    profile::profile,
    meters::{
        get_my_meters, register_meter, 
        get_registered_meters_filtered, update_meter_status, create_reading,
        get_my_readings,
    },
    wallets::token_balance,
    status::{system_status, meter_status, readiness_probe, liveness_probe},
};

// ============================================================================
// V1 RESTful API Routes (New)
// ============================================================================

/// Build V1 auth routes: POST /api/v1/auth/token, GET /api/v1/auth/verify, password reset
pub fn v1_auth_routes() -> Router<AppState> {
    Router::new()
        .route("/token", post(login))  // POST /api/v1/auth/token
        .route("/verify", get(verify_email))  // GET /api/v1/auth/verify
        .route("/forgot-password", post(forgot_password))  // POST /api/v1/auth/forgot-password
        .route("/reset-password", post(reset_password))  // POST /api/v1/auth/reset-password
        .route("/change-password", post(change_password))  // POST /api/v1/auth/change-password
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
        .route("/readings", get(get_my_readings))  // GET /api/v1/meters/readings
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
        .route("/ready", get(readiness_probe))  // GET /api/v1/status/ready
        .route("/live", get(liveness_probe))  // GET /api/v1/status/live
}

