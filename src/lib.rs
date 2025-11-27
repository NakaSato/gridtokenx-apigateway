pub mod auth;
pub mod config;
pub mod database;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod utils;

pub use config::Config;
pub use error::ApiError;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub timescale_db: Option<sqlx::PgPool>,
    pub redis: redis::Client,
    pub config: Config,
    pub jwt_service: auth::jwt::JwtService,
    pub api_key_service: auth::jwt::ApiKeyService,
    pub email_service: Option<services::EmailService>,
    pub blockchain_service: services::BlockchainService,
    pub wallet_service: services::WalletService,
    pub meter_service: services::MeterService,
    pub meter_verification_service: services::MeterVerificationService,
    pub erc_service: services::ErcService,
    pub order_matching_engine: services::OrderMatchingEngine,
    pub market_clearing_engine: services::MarketClearingEngine,
    pub market_clearing_service: services::MarketClearingService,
    pub settlement_service: services::SettlementService,
    pub websocket_service: services::WebSocketService,
    // TODO: Re-enable when ValidationServices implementation is available
    // pub transaction_coordinator: services::TransactionCoordinator,
    pub meter_polling_service: services::MeterPollingService,
    pub event_processor_service: services::EventProcessorService,
    pub health_checker: services::HealthChecker,
    pub audit_logger: services::AuditLogger,
    pub cache_service: services::CacheService,
}
