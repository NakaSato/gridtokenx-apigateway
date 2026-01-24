//! Application state shared across all handlers.
//!
//! Minimal version for testing Simulator → Gateway → Anchor flow.

use crate::auth::jwt::{ApiKeyService, JwtService};
use crate::config::Config;
use crate::services;

/// Application state shared across handlers.
///
/// Minimal version with only essential services for blockchain testing.
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL connection pool for primary database operations
    pub db: sqlx::PgPool,
    /// Redis client for caching and pub/sub
    pub redis: redis::Client,
    /// Application configuration
    pub config: Config,
    /// JWT authentication service
    pub jwt_service: JwtService,
    /// API key authentication service
    pub api_key_service: ApiKeyService,
    /// Authentication service
    pub auth: services::AuthService,
    /// Optional email service
    pub email_service: Option<services::EmailService>,
    /// Solana blockchain interaction service
    pub blockchain_service: services::BlockchainService,
    /// Wallet management service
    pub wallet_service: services::WalletService,
    /// WebSocket service for real-time updates
    pub websocket_service: services::WebSocketService,
    /// Redis cache service
    pub cache_service: services::CacheService,
    /// Health check service
    pub health_checker: services::HealthChecker,

    // P2P Trading Services
    pub audit_logger: services::AuditLogger,
    pub market_clearing: services::MarketClearingService,
    pub settlement: services::SettlementService,
    pub market_clearing_engine: services::OrderMatchingEngine,
    pub futures_service: services::FuturesService,
    pub dashboard_service: services::DashboardService,
    pub event_processor: services::EventProcessorService,
    pub price_monitor: services::PriceMonitor,
    pub recurring_scheduler: services::RecurringScheduler,
    pub reading_processor: services::reading_processor::ReadingProcessorService,
    pub webhook_service: services::WebhookService,
    pub erc_service: services::ErcService,
    pub notification_dispatcher: services::NotificationDispatcher,
    pub blockchain_task_service: services::BlockchainTaskService,
    
    /// Prometheus metrics handle
    pub metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
    /// HTTP Client for external requests (Simulator, etc.)
    pub http_client: reqwest::Client,
}


// Implement FromRef for services that need to be extracted from AppState
impl axum::extract::FromRef<AppState> for services::WebSocketService {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.websocket_service.clone()
    }
}

impl axum::extract::FromRef<AppState> for services::HealthChecker {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.health_checker.clone()
    }
}

impl axum::extract::FromRef<AppState> for services::DashboardService {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.dashboard_service.clone()
    }
}
