//! Services module - Minimal build for testing Simulator → Gateway → Anchor flow

// Core services that don't use SQLx macros heavily
pub mod auth;
pub mod blockchain;
pub mod cache;
pub mod email;
pub mod health_check;
pub mod wallet;
pub mod websocket;

// Enabled Services for P2P Trading
pub mod audit_logger;
pub mod market_clearing;
pub mod settlement;
pub mod order_matching_engine;
pub mod futures;
pub mod dashboard;
pub mod event_processor;
pub mod transaction;
pub mod validation;
pub mod webhook;
pub mod erc;
pub mod grid_topology;
pub mod notification;
pub mod price_monitor;
pub mod reading_processor;
pub mod recurring_scheduler;
pub mod notification_dispatcher;
pub mod kafka;

pub mod meter_analyzer;
pub mod meter;

// Re-exports
pub use auth::AuthService;
pub use blockchain::BlockchainService;
pub use cache::CacheService;
pub use email::EmailService;
pub use health_check::HealthChecker;
pub use wallet::WalletService;
pub use websocket::WebSocketService;

pub use audit_logger::{AuditLogger, AuditEvent};
pub use market_clearing::MarketClearingService;
pub use settlement::SettlementService;
pub use order_matching_engine::OrderMatchingEngine;
pub use futures::FuturesService;
pub use dashboard::DashboardService;
pub use event_processor::EventProcessorService;
pub use webhook::WebhookService;
pub use erc::ErcService;
pub use grid_topology::GridTopologyService;
pub use notification::NotificationService;
pub use price_monitor::{PriceMonitor, PriceMonitorConfig};
pub use recurring_scheduler::{RecurringScheduler, RecurringSchedulerConfig};
pub use notification_dispatcher::{NotificationDispatcher, NotificationDispatcherConfig};
pub use kafka::KafkaConsumerService;
