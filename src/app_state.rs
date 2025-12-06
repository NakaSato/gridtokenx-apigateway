//! Application state shared across all handlers.
//!
//! This module defines the `AppState` struct which holds all shared services
//! and connections needed by the API gateway.

use crate::auth::jwt::{ApiKeyService, JwtService};
use crate::config::Config;
use crate::services;

/// Application state shared across handlers.
///
/// Contains all database connections, service instances, and configuration
/// needed to process API requests.
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL connection pool for primary database operations
    pub db: sqlx::PgPool,
    /// Optional TimescaleDB connection pool for time-series data
    pub timescale_db: Option<sqlx::PgPool>,
    /// Redis client for caching and pub/sub
    pub redis: redis::Client,
    /// Application configuration
    pub config: Config,
    /// JWT authentication service
    pub jwt_service: JwtService,
    /// API key authentication service
    pub api_key_service: ApiKeyService,
    /// Optional email service (disabled if SMTP not configured)
    pub email_service: Option<services::EmailService>,
    /// Solana blockchain interaction service
    pub blockchain_service: services::BlockchainService,
    /// Wallet management service
    pub wallet_service: services::WalletService,
    /// Smart meter management service
    pub meter_service: services::MeterService,
    /// Meter verification and security service
    pub meter_verification_service: services::MeterVerificationService,
    /// Energy Renewable Certificate (ERC) service
    pub erc_service: services::ErcService,
    /// Real-time order matching engine
    pub order_matching_engine: services::OrderMatchingEngine,
    /// Market clearing engine for P2P trading
    pub market_clearing_engine: services::MarketClearingEngine,
    /// Market clearing service for epoch management
    pub market_clearing_service: services::MarketClearingService,
    /// Trade settlement service
    pub settlement_service: services::SettlementService,
    /// WebSocket service for real-time updates
    pub websocket_service: services::WebSocketService,
    /// Meter polling service for automated readings
    pub meter_polling_service: services::MeterPollingService,
    /// Blockchain event processor service
    pub event_processor_service: services::EventProcessorService,
    /// Health check service
    pub health_checker: services::HealthChecker,
    /// Audit logging service
    pub audit_logger: services::AuditLogger,
    /// Redis cache service
    pub cache_service: services::CacheService,
    /// Dashboard metrics service
    pub dashboard_service: services::DashboardService,
    /// Automated Market Maker service
    pub amm_service: services::AmmService,
    /// Wallet audit logger for security monitoring
    pub wallet_audit_logger: services::WalletAuditLogger,
}

// Implement FromRef for services that need to be extracted from AppState
impl axum::extract::FromRef<AppState> for services::DashboardService {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.dashboard_service.clone()
    }
}

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
