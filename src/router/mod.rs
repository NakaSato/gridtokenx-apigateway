//! Router configuration module - Minimal build
//!
//! Only includes health check and meter stub routes for testing.

use axum::{routing::get, Router, extract::{State, WebSocketUpgrade}, response::IntoResponse};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::meter_stub;
use crate::services::WebSocketService;

/// Build the minimal application router for testing.
pub fn build_router(app_state: AppState) -> Router {
    // Health check routes
    let health = Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check));

    // WebSocket endpoint for real-time updates
    let ws = Router::new()
        .route("/ws", get(websocket_handler));

    // Meter stub routes (publicly accessible for simulator testing)
    let meters = meter_stub::meter_routes();
    let auth = crate::handlers::auth_stub::auth_routes();

    health
        .merge(ws)
        .nest("/api/meters", meters)
        .nest("/api/auth", auth)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::with_status_code(
                    axum::http::StatusCode::REQUEST_TIMEOUT,
                    std::time::Duration::from_secs(900),
                ))
                .layer(CorsLayer::permissive()),
        )
        .with_state(app_state)
}

/// Simple health check endpoint
async fn health_check() -> &'static str {
    "OK - Minimal Gateway Running"
}

/// WebSocket handler for real-time market and minting updates
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(websocket_service): State<WebSocketService>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        websocket_service.register_client(socket).await;
    })
}
