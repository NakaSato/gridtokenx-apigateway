//! Application startup and initialization logic.
//!
//! This module handles:
//! - Database connection setup
//! - Redis connection setup
//! - Service initialization
//! - Background task spawning

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::app_state::AppState;
use crate::auth::jwt::{ApiKeyService, JwtService};
use crate::config::Config;
use crate::database;
use crate::services;

/// Initialize all application services and create the AppState.
pub async fn initialize_app(config: &Config) -> Result<AppState> {
    // Setup database connections
    let db_pool = database::setup_database(&config.database_url).await?;
    info!("PostgreSQL connection established");

    let timescale_pool = database::setup_timescale_database(&config.influxdb_url).await?;
    if timescale_pool.is_some() {
        info!("TimescaleDB connection established");
    }

    // Run database migrations
    database::run_migrations(&db_pool).await?;
    info!("✅ Database migrations completed successfully");

    // Setup Redis connection
    let redis_client = setup_redis(config).await?;

    // Initialize authentication services
    let jwt_service = JwtService::new()?;
    let api_key_service = ApiKeyService::new()?;
    info!("Authentication services initialized");

    // Initialize email service (optional)
    let email_service = initialize_email_service(config);

    // Initialize blockchain service
    let blockchain_service = services::BlockchainService::new(
        config.solana_rpc_url.clone(),
        "localnet".to_string(),
        config.solana_programs.clone(),
    )?;
    info!("Blockchain service initialized");

    // Initialize wallet service
    let wallet_service = if let Ok(path) = std::env::var("AUTHORITY_WALLET_PATH") {
        info!("Initializing wallet service with authority path: {}", path);
        services::WalletService::with_path(&config.solana_rpc_url, path)
    } else {
        services::WalletService::new(&config.solana_rpc_url)
    };
    initialize_wallet(&wallet_service).await;

    // Initialize meter service
    let meter_service = services::MeterService::new(db_pool.clone());
    info!("Meter service initialized");

    // Initialize meter verification service
    let meter_verification_service = services::MeterVerificationService::new(db_pool.clone());
    info!("Meter verification service initialized");

    // Initialize ERC service
    let erc_service = services::ErcService::new(db_pool.clone(), blockchain_service.clone());
    info!("ERC service initialized");

    // Initialize settlement service
    let settlement_service =
        services::SettlementService::new(db_pool.clone(), blockchain_service.clone());
    info!("✅ Settlement service initialized");

    // Initialize metrics
    services::transaction_metrics::init_metrics();
    info!("Prometheus metrics initialized");

    // Initialize market clearing service
    let market_clearing_service =
        services::MarketClearingService::new(db_pool.clone(), blockchain_service.clone());
    info!("✅ Market clearing service initialized for epoch management");

    // Initialize WebSocket service
    let websocket_service = services::WebSocketService::new();
    info!("✅ WebSocket service initialized for real-time updates");

    // Initialize order matching engine
    let order_matching_engine = services::OrderMatchingEngine::new(db_pool.clone())
        .with_websocket(websocket_service.clone())
        .with_settlement_service(settlement_service.clone());
    info!("Order matching engine initialized");

    // Initialize market clearing engine
    let market_clearing_engine =
        services::MarketClearingEngine::new(db_pool.clone(), redis_client.clone())
            .with_websocket(websocket_service.clone())
            .with_settlement_service(settlement_service.clone());
    info!("✅ Market clearing engine initialized with WebSocket support");

    // Load active orders into order book
    match market_clearing_engine.load_order_book().await {
        Ok(count) => info!("Loaded {} active orders into order book", count),
        Err(e) => warn!("Failed to load order book: {}", e),
    }

    // Initialize health checker
    let health_checker = services::HealthChecker::new(
        db_pool.clone(),
        redis_client.clone(),
        config.solana_rpc_url.clone(),
    );
    info!("Health checker initialized");

    // Initialize audit logger
    let audit_logger = services::AuditLogger::new(db_pool.clone());
    info!("✅ Audit logger initialized for security event tracking");

    // Initialize cache service
    let cache_service = services::CacheService::new(&config.redis_url).await?;
    info!("✅ Cache service initialized for performance optimization");

    // Initialize event processor service
    let event_processor_service = services::EventProcessorService::new(
        Arc::new(db_pool.clone()),
        config.solana_rpc_url.clone(),
        config.event_processor.clone(),
        config.energy_token_mint.clone(),
    );
    info!("✅ Event processor service initialized for blockchain event sync");

    // Initialize dashboard service
    let dashboard_service =
        services::DashboardService::new(health_checker.clone(), event_processor_service.clone());
    info!("✅ Dashboard service initialized");

    // Initialize AMM service
    let amm_service = services::AmmService::new(db_pool.clone());
    info!("✅ AMM service initialized");

    // Initialize meter polling service
    let meter_polling_service = services::MeterPollingService::new(
        Arc::new(db_pool.clone()),
        Arc::new(blockchain_service.clone()),
        Arc::new(meter_service.clone()),
        Arc::new(websocket_service.clone()),
        config.tokenization.clone(),
    );

    // Initialize wallet audit logger
    let wallet_audit_logger = services::WalletAuditLogger::new(db_pool.clone());
    info!("✅ Wallet audit logger initialized for security monitoring");

    // Create application state
    let app_state = AppState {
        db: db_pool,
        timescale_db: timescale_pool,
        redis: redis_client,
        config: config.clone(),
        jwt_service,
        api_key_service,
        email_service,
        blockchain_service,
        wallet_service,
        meter_service,
        meter_verification_service,
        erc_service,
        order_matching_engine,
        market_clearing_engine,
        market_clearing_service,
        settlement_service,
        websocket_service,
        meter_polling_service,
        event_processor_service,
        health_checker,
        audit_logger,
        cache_service,
        dashboard_service,
        amm_service,
        wallet_audit_logger,
    };

    Ok(app_state)
}

/// Setup Redis connection with authentication support.
async fn setup_redis(config: &Config) -> Result<redis::Client> {
    let redis_client = redis::Client::open(config.redis_url.as_str())?;

    // Test Redis connection and validate authentication
    match redis_client.get_multiplexed_async_connection().await {
        Ok(mut conn) => {
            use redis::AsyncCommands;
            match conn.get::<&str, Option<String>>("health_check").await {
                Ok(_) => {
                    let auth_status = if config.redis_url.contains("@") {
                        "✅ Redis connection established (authenticated)"
                    } else {
                        "⚠️  Redis connection established (no authentication - consider adding password)"
                    };
                    info!("{}", auth_status);

                    if config.environment == "production" && !config.redis_url.contains("@") {
                        warn!(
                            "🚨 SECURITY WARNING: Redis connection in production is not authenticated!"
                        );
                    }
                }
                Err(e) => {
                    error!("Redis connection test failed: {}", e);
                    return Err(anyhow::anyhow!("Redis connection test failed: {}", e));
                }
            }
        }
        Err(e) => {
            error!("Failed to establish Redis connection: {}", e);

            if e.to_string().contains("NOAUTH") {
                error!("Redis authentication failed. Please check your REDIS_URL format:");
                error!("  Correct format: redis://:password@host:port");
                error!("  Current URL: {}", config.redis_url);
            } else if e.to_string().contains("Connection refused") {
                error!(
                    "Redis server is not running or not accessible at: {}",
                    config.redis_url
                );
            }

            return Err(anyhow::anyhow!("Redis connection failed: {}", e));
        }
    }

    Ok(redis_client)
}

/// Initialize email service (optional).
fn initialize_email_service(config: &Config) -> Option<services::EmailService> {
    match services::EmailService::new(&config.email) {
        Ok(service) => {
            info!("Email service initialized successfully");
            Some(service)
        }
        Err(e) => {
            warn!(
                "Email service initialization failed: {}. Email verification will be disabled.",
                e
            );
            None
        }
    }
}

/// Initialize wallet service and load authority wallet.
async fn initialize_wallet(wallet_service: &services::WalletService) {
    match wallet_service.initialize_authority().await {
        Ok(()) => {
            if let Ok(pubkey) = wallet_service.get_authority_pubkey_string().await {
                info!("Authority wallet loaded: {}", pubkey);
            }
        }
        Err(e) => {
            warn!(
                "Failed to load authority wallet: {}. Token minting will not be available.",
                e
            );
        }
    }
}

/// Spawn background tasks for the application.
pub async fn spawn_background_tasks(app_state: &AppState, _config: &Config) {
    // Start the order matching engine
    app_state.order_matching_engine.start().await;
    info!("✅ Automated order matching engine started");

    // Start settlement processing loop
    let settlement_service = app_state.settlement_service.clone();
    tokio::spawn(async move {
        info!("🚀 Settlement processing loop started");
        loop {
            if let Err(e) = settlement_service.process_pending_settlements().await {
                error!("Error in settlement loop: {}", e);
            }
            if let Err(e) = settlement_service.retry_failed_settlements(3).await {
                error!("Error retrying settlements: {}", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    // Initialize and start epoch scheduler
    let epoch_scheduler = Arc::new(services::EpochScheduler::new(
        app_state.db.clone(),
        services::EpochConfig::default(),
        app_state.blockchain_service.clone(),
    ));
    info!("✅ Epoch scheduler initialized (15-minute intervals)");

    match epoch_scheduler.start().await {
        Ok(_) => info!("🚀 Epoch scheduler started successfully"),
        Err(e) => warn!("⚠️  Failed to start epoch scheduler: {}", e),
    }

    // Start meter polling service
    let meter_polling_service = app_state.meter_polling_service.clone();
    tokio::spawn(async move {
        info!("🚀 Meter polling service started");
        meter_polling_service.start().await;
    });

    // Start event processor service
    let event_processor_service = app_state.event_processor_service.clone();
    tokio::spawn(async move {
        info!("🚀 Event processor service started");
        event_processor_service.start().await;
    });
}

/// Wait for shutdown signal (SIGTERM or SIGINT).
pub async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                error!("Failed to install signal handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal, shutting down gracefully");
        },
        _ = terminate => {
            info!("Received SIGTERM signal, shutting down gracefully");
        },
    }
}
