//! Public routes that don't require authentication.
//!
//! Includes: health checks, authentication endpoints, public market data, WebSocket, Swagger UI.

use axum::{routing::{get, post}, Router};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::app_state::AppState;
use crate::handlers::{
    self, auth as auth_handlers, epochs, health, user_management, wallet_auth,
};

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(info(title = "GridTokenX API", version = "1.0.0"))]
struct ApiDoc;

/// Build public routes that don't require authentication.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        // Health check routes
        .route("/health", get(health::health_check))
        .route("/metrics", get(handlers::metrics::get_prometheus_metrics))
        .route(
            "/health/metrics",
            get(handlers::metrics::get_health_with_metrics),
        )
        .route(
            "/health/event-processor",
            get(health::get_event_processor_stats),
        )
        .route(
            "/api/dashboard/metrics",
            get(handlers::dashboard::get_dashboard_metrics),
        )
        // Authentication routes
        .route("/api/auth/login", post(auth_handlers::login))
        .route("/api/auth/register", post(user_management::register))
        .route(
            "/api/auth/verify-email",
            get(handlers::email_verification::verify_email),
        )
        .route(
            "/api/auth/resend-verification",
            post(handlers::email_verification::resend_verification),
        )
        // Wallet authentication routes
        .route(
            "/api/auth/wallet/login",
            post(wallet_auth::login_with_wallet),
        )
        .route(
            "/api/auth/wallet/register",
            post(wallet_auth::register_with_wallet),
        )
        // Public market endpoints
        .route("/api/market/epoch", get(epochs::get_current_epoch))
        .route("/api/market/epoch/status", get(epochs::get_epoch_status))
        .route(
            "/api/market/orderbook",
            get(handlers::energy_trading::get_orderbook),
        )
        .route(
            "/api/market/stats",
            get(handlers::energy_trading::get_market_stats),
        )
        // WebSocket endpoints
        .route(
            "/api/market/ws",
            get(handlers::websocket::market_websocket_handler),
        )
        .route("/ws", get(handlers::websocket::websocket_handler))
        // Swagger UI
        .merge(
            SwaggerUi::new("/api/docs").url("/api/docs/openapi.json", ApiDoc::openapi()),
        )
}
