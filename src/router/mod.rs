//! Router configuration module - RESTful v1 API
//!
//! Supports both v1 RESTful API and legacy routes for backward compatibility.

use axum::{routing::get, Router, extract::{State, WebSocketUpgrade}, response::IntoResponse};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    // V1 RESTful routes
    v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes,
    // Legacy routes
    auth_routes, token_routes, user_meter_routes, meter_info_routes, meter_routes,
};
use crate::services::WebSocketService;

/// Build the application router with both v1 and legacy routes.
pub fn build_router(app_state: AppState) -> Router {
    // Health check routes (always at root)
    let health = Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check));

    // WebSocket endpoint
    let ws = Router::new()
        .route("/ws", get(websocket_handler));

    // =========================================================================
    // V1 RESTful API Routes (New)
    // =========================================================================
    let v1_api = Router::new()
        .nest("/auth", v1_auth_routes())       // POST /api/v1/auth/token, GET /api/v1/auth/verify
        .nest("/users", v1_users_routes())     // POST /api/v1/users, GET /api/v1/users/me
        .nest("/meters", v1_meters_routes())   // POST /api/v1/meters, PATCH /api/v1/meters/{serial}
        .nest("/wallets", v1_wallets_routes()) // GET /api/v1/wallets/{address}/balance
        .nest("/status", v1_status_routes());  // GET /api/v1/status

    // =========================================================================
    // Legacy Routes (Backward Compatibility - Deprecated)
    // =========================================================================
    let legacy_meters = meter_routes();
    let legacy_auth = auth_routes();
    let legacy_tokens = token_routes();
    let legacy_user = user_meter_routes();
    let legacy_meter_info = meter_info_routes();

    health
        .merge(ws)
        // V1 API
        .nest("/api/v1", v1_api)
        // Legacy routes (deprecated)
        .nest("/api/meters", legacy_meters)
        .nest("/api/meters", legacy_meter_info)
        .nest("/api/auth", legacy_auth)
        .nest("/api/tokens", legacy_tokens)
        .nest("/api/user", legacy_user)
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
    "OK - Gateway Running (Real Data Mode)"
}

/// WebSocket handler for real-time updates
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(websocket_service): State<WebSocketService>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        websocket_service.register_client(socket).await;
    })
}
