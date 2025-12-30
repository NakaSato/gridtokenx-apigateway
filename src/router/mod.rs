//! Router configuration module - RESTful v1 API
//!
//! Supports both v1 RESTful API and legacy routes for backward compatibility.

use axum::{routing::{get, post}, Router, extract::{State, WebSocketUpgrade}, response::IntoResponse, middleware};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod dev;
pub mod public;

use crate::app_state::AppState;
use crate::handlers::{
    // V1 RESTful routes
    v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes,
    v1_trading_routes, v1_dashboard_routes,
};
use crate::services::WebSocketService;
use crate::auth::middleware::auth_middleware;
use crate::middleware::{metrics_middleware, active_requests_middleware};

/// OpenAPI documentation for GridTokenX API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "GridTokenX API",
        version = "1.0.0",
        description = "GridTokenX Energy Trading Platform API"
    ),
    tags(
        (name = "auth", description = "Authentication endpoints"),
        (name = "users", description = "User management"),
        (name = "trading", description = "P2P Energy Trading"),
        (name = "meters", description = "Smart Meter management"),
        (name = "dev", description = "Developer tools")
    ),
    paths(
        crate::handlers::auth::login::login,
        crate::handlers::auth::login::verify_email,
        crate::handlers::auth::registration::register,
        crate::handlers::auth::registration::resend_verification,
        crate::handlers::auth::profile::profile,
        crate::handlers::auth::password_reset::forgot_password,
        crate::handlers::auth::password_reset::reset_password,
        crate::handlers::auth::password_reset::change_password,
        crate::handlers::auth::meters::get_my_meters,
        crate::handlers::auth::meters::get_registered_meters,
        crate::handlers::auth::meters::register_meter,
        crate::handlers::auth::meters::verify_meter,
        crate::handlers::auth::meters::get_registered_meters_filtered,
        crate::handlers::auth::meters::update_meter_status,
        crate::handlers::auth::meters::create_reading,
        crate::handlers::auth::meters::get_my_readings,
        crate::handlers::trading::orders::create::create_order,
        crate::handlers::trading::orders::queries::get_user_orders,
        crate::handlers::trading::orders::management::cancel_order,
        crate::handlers::trading::orders::management::update_order,
        crate::handlers::trading::orders::queries::get_order_book,
        crate::handlers::trading::orders::queries::get_my_trades,
        crate::handlers::trading::orders::queries::get_token_balance,
        crate::handlers::trading::blockchain::get_blockchain_market_data,
        crate::handlers::trading::blockchain::match_blockchain_orders,
        crate::handlers::auth::wallets::token_balance,
        crate::handlers::auth::status::system_status,
        crate::handlers::auth::status::meter_status,
        crate::handlers::auth::status::readiness_probe,
        crate::handlers::auth::status::liveness_probe,
        crate::handlers::analytics::market::get_market_analytics,
        crate::handlers::analytics::user::get_user_trading_stats,
        crate::handlers::analytics::user::get_user_wealth_history,
        crate::handlers::analytics::admin::get_admin_stats,
        crate::handlers::analytics::admin::get_admin_activity,
        crate::handlers::analytics::admin::get_system_health,
        crate::handlers::futures::get_products,
        crate::handlers::futures::create_order,
        crate::handlers::futures::get_my_orders,
        crate::handlers::futures::get_positions,
        crate::handlers::futures::close_position,
        crate::handlers::futures::get_candles,
        crate::handlers::futures::get_order_book,
        crate::handlers::websocket::handlers::websocket_handler,
        crate::handlers::websocket::handlers::market_websocket_handler,
        crate::handlers::websocket::handlers::websocket_stats,
        crate::handlers::dev::metrics::get_metrics,
        crate::handlers::dashboard::get_dashboard_metrics,
    ),
    components(
        schemas(
            crate::handlers::auth::types::LoginRequest,
            crate::handlers::auth::types::AuthResponse,
            crate::handlers::auth::types::UserResponse,
            crate::handlers::auth::types::RegistrationRequest,
            crate::handlers::auth::types::RegistrationResponse,
            crate::handlers::auth::types::VerifyEmailRequest,
            crate::handlers::auth::types::VerifyEmailResponse,
            crate::handlers::auth::types::ResendVerificationRequest,
            crate::handlers::auth::types::ForgotPasswordRequest,
            crate::handlers::auth::types::ResetPasswordRequest,
            crate::handlers::auth::types::ChangePasswordRequest,
            crate::handlers::auth::types::MeterResponse,
            crate::handlers::auth::types::RegisterMeterRequest,
            crate::handlers::auth::types::RegisterMeterResponse,
            crate::handlers::auth::types::VerifyMeterRequest,
            crate::handlers::auth::types::UpdateMeterStatusRequest,
            crate::handlers::auth::types::CreateReadingRequest,
            crate::handlers::auth::types::CreateReadingResponse,
            crate::handlers::auth::types::MeterReadingResponse,
            crate::models::trading::TradingOrder,
            crate::models::trading::CreateOrderRequest,
            crate::models::trading::UpdateOrderRequest,
            crate::models::trading::MarketData,
            crate::models::trading::OrderBook,
            crate::models::trading::Trade,
            crate::handlers::trading::types::TradingOrdersResponse,
            crate::handlers::trading::types::CreateOrderResponse,
            crate::handlers::trading::types::TradingStats,
            crate::handlers::trading::types::BlockchainMarketData,
            crate::handlers::trading::types::CreateBlockchainOrderRequest,
            crate::handlers::trading::types::CreateBlockchainOrderResponse,
            crate::handlers::trading::types::MatchOrdersResponse,
            crate::handlers::trading::types::MarketStats,
            crate::handlers::trading::orders::queries::TradeRecord,
            crate::handlers::trading::orders::queries::TradeHistoryResponse,
            crate::handlers::trading::orders::queries::TokenBalanceResponse,
            crate::database::schema::types::OrderSide,
            crate::database::schema::types::OrderType,
            crate::database::schema::types::OrderStatus,
            crate::handlers::auth::status::HealthResponse,
            crate::handlers::auth::status::ServiceStatus,
            crate::handlers::auth::status::ServiceHealth,
            crate::handlers::auth::status::StatusResponse,
            crate::handlers::auth::status::MeterStatusResponse,
            crate::handlers::auth::status::MeterCounts,
            crate::handlers::auth::status::ReadinessResponse,
            crate::handlers::auth::status::CheckResult,
            crate::handlers::auth::status::LivenessResponse,
            crate::handlers::analytics::types::MarketAnalytics,
            crate::handlers::analytics::types::MarketOverview,
            crate::handlers::analytics::types::TradingVolume,
            crate::handlers::analytics::types::PriceStatistics,
            crate::handlers::analytics::types::EnergySourceStats,
            crate::handlers::analytics::types::TraderStats,
            crate::handlers::analytics::types::UserTradingStats,
            crate::handlers::analytics::types::SellerStats,
            crate::handlers::analytics::types::BuyerStats,
            crate::handlers::analytics::types::OverallUserStats,
            crate::handlers::analytics::types::UserWealthHistory,
            crate::handlers::analytics::types::WealthPoint,
            crate::handlers::analytics::admin::AdminStatsResponse,
            crate::services::audit_logger::types::AuditEventRecord,
            crate::services::health_check::types::DetailedHealthStatus,
            crate::services::health_check::types::DependencyHealth,
            crate::services::health_check::types::HealthCheckStatus,
            crate::services::health_check::types::SystemMetrics,
            crate::handlers::futures::CreateFuturesOrderRequest,
            crate::services::futures::FuturesProduct,
            crate::services::futures::FuturesPosition,
            crate::services::futures::Candle,
            crate::services::futures::OrderBook,
            crate::services::futures::OrderBookEntry,
            crate::services::futures::FuturesOrder,
            crate::services::dashboard::types::DashboardMetrics,
            crate::services::event_processor::types::EventProcessorStats,
            crate::handlers::trading::types::OrderBookResponse,
            crate::handlers::trading::types::OrderBookEntry,
        )
    )
)]
struct ApiDoc;

/// Build the application router with both v1 and legacy routes.
pub fn build_router(app_state: AppState) -> Router {
    // Health check routes (always at root)
    let health = Router::new()
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/metrics", get(crate::handlers::dev::metrics::get_metrics));

    // WebSocket endpoint
    let ws = Router::new()
        .route("/ws", get(websocket_handler));

    // Swagger UI
    let swagger = SwaggerUi::new("/api/docs")
        .url("/api/docs/openapi.json", ApiDoc::openapi());

    // =========================================================================
    // V1 RESTful API Routes (New)
    // =========================================================================
    let trading_routes = v1_trading_routes();
        // .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware));

    let futures_routes = crate::handlers::futures::routes()
        .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware));

    let analytics_routes = crate::handlers::analytics::routes()
        .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware));

    let meters_routes = v1_meters_routes()
        .layer(middleware::from_fn_with_state(app_state.clone(), auth_middleware));

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/meters", get(crate::handlers::auth::meters::public_get_meters))
        .route("/grid-status", get(crate::handlers::auth::meters::public_grid_status))
        .route("/meters/batch/readings", post(crate::handlers::auth::meters::create_batch_readings));

    let v1_api = Router::new()
        .nest("/auth", v1_auth_routes())       // POST /api/v1/auth/token, GET /api/v1/auth/verify
        .nest("/users", v1_users_routes())     // POST /api/v1/users, GET /api/v1/users/me
        .nest("/meters", meters_routes)        // POST /api/v1/meters, auth required for minting
        .nest("/wallets", v1_wallets_routes()) // GET /api/v1/wallets/{address}/balance
        .nest("/status", v1_status_routes())   // GET /api/v1/status
        .nest("/trading", trading_routes)      // POST /api/v1/trading/orders
        .nest("/futures", futures_routes)      // /api/v1/futures
        .nest("/analytics", analytics_routes)  // /api/v1/analytics
        .nest("/dashboard", v1_dashboard_routes()) // /api/v1/dashboard/metrics
        .nest("/dev", dev::dev_routes())       // POST /api/v1/dev/faucet
        .nest("/public", public_routes)        // GET /api/v1/public/meters (no auth)
        .route("/rpc", axum::routing::post(crate::handlers::rpc::rpc_handler)); // /api/v1/rpc

    health
        .merge(ws)
        .merge(swagger)  // Swagger UI at /api/docs
        // V1 API
        .nest("/api/v1", v1_api)
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn(metrics_middleware))
                .layer(middleware::from_fn(active_requests_middleware))
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::with_status_code(
                    axum::http::StatusCode::REQUEST_TIMEOUT,
                    std::time::Duration::from_secs(900),
                ))
                .layer({
                    let allowed_origins = app_state.config.cors_allowed_origins.clone();
                    CorsLayer::new()
                        .allow_origin(tower_http::cors::AllowOrigin::predicate(
                            move |origin: &axum::http::HeaderValue, _request_parts: &axum::http::request::Parts| {
                                let origin_str = origin.to_str().unwrap_or("");
                                allowed_origins.iter().any(|allowed| {
                                    origin_str == allowed || origin_str.starts_with(allowed)
                                })
                            },
                        ))
                        .allow_methods([
                            axum::http::Method::GET,
                            axum::http::Method::POST,
                            axum::http::Method::PUT,
                            axum::http::Method::PATCH,
                            axum::http::Method::DELETE,
                            axum::http::Method::OPTIONS,
                        ])
                        .allow_headers([
                            axum::http::header::AUTHORIZATION,
                            axum::http::header::CONTENT_TYPE,
                            axum::http::header::ACCEPT,
                        ])
                        .allow_credentials(true)
                }),
        )
        .with_state(app_state)
}

/// Simple health check endpoint
async fn health_check(
    State(app_state): State<AppState>,
) -> axum::Json<crate::services::health_check::DetailedHealthStatus> {
    let status = app_state.health_checker.perform_health_check().await;
    axum::Json(status)
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

