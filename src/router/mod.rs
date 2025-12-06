//! Router configuration module.
//!
//! This module organizes all API routes into logical groups:
//! - `public`: Unauthenticated routes (health, auth, public market data)
//! - `protected`: Authenticated routes (user operations, trading, meters)
//! - `admin`: Admin-only routes (system management, analytics)

mod admin;
mod protected;
mod public;

pub use admin::admin_routes;
pub use protected::protected_routes;
pub use public::public_routes;

use axum::Router;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::middleware;

/// Build the complete application router with all routes and middleware.
pub fn build_router(app_state: AppState) -> Router {
    let public = public_routes();
    let protected = protected_routes(app_state.clone());

    public
        .merge(protected)
        .layer(
            ServiceBuilder::new()
                .layer(axum::middleware::from_fn(
                    middleware::json_validation_middleware,
                ))
                .layer(axum::middleware::from_fn(middleware::add_security_headers))
                .layer(axum::middleware::from_fn(middleware::metrics_middleware))
                .layer(axum::middleware::from_fn(
                    middleware::active_requests_middleware,
                ))
                .layer(axum::middleware::from_fn(
                    middleware::request_logger_middleware,
                ))
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::with_status_code(
                    axum::http::StatusCode::REQUEST_TIMEOUT,
                    std::time::Duration::from_secs(900),
                ))
                .layer(CorsLayer::permissive()),
        )
        .with_state(app_state)
}
