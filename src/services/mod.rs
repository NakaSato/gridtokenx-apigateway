// Business logic services
// Authentication, blockchain client, trading engine, etc.

pub mod audit_logger;
pub mod blockchain_service;
pub mod cache_service;
pub mod email_service;
pub mod email_templates;
pub mod epoch_scheduler;
pub mod erc_service;
pub mod health_check;
pub mod market_clearing;
pub mod market_clearing_service;
// pub mod meter_polling_service; // TODO: Fix compilation errors before re-enabling
pub mod meter_service;
pub mod meter_verification_service;
pub mod order_matching_engine;
pub mod priority_fee_service;
pub mod settlement_service;
pub mod token_service;
pub mod transaction_service;
pub mod wallet_service;
pub mod websocket_service;

pub use audit_logger::{AuditEvent, AuditEventRecord, AuditLogger};
pub use blockchain_service::BlockchainService;
pub use cache_service::CacheService;
pub use email_service::EmailService;
pub use epoch_scheduler::{EpochConfig, EpochScheduler};
pub use erc_service::ErcService;
pub use health_check::HealthChecker;
pub use market_clearing::{ClearingPrice, MarketClearingEngine};
pub use market_clearing_service::MarketClearingService;
// pub use meter_polling_service::{MeterPollingService, MintResult}; // TODO: Fix compilation errors before re-enabling
pub use meter_service::MeterService;
pub use meter_verification_service::MeterVerificationService;
pub use order_matching_engine::OrderMatchingEngine;
pub use settlement_service::SettlementService;
pub use token_service::TokenService;
pub use wallet_service::WalletService;
pub use websocket_service::WebSocketService;
