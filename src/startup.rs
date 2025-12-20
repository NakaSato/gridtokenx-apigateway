//! Application startup and initialization logic - Minimal build
//!
//! Only initializes essential services for Simulator → Gateway → Anchor testing.

use anyhow::Result;
use tracing::{error, info, warn};

use crate::app_state::AppState;
use crate::auth::jwt::{ApiKeyService, JwtService};
use crate::config::Config;
use crate::database;
use crate::services;

/// Initialize minimal application services and create the AppState.
pub async fn initialize_app(config: &Config) -> Result<AppState> {
    info!("🚀 Starting minimal Gateway for Simulator → Anchor testing");

    // Initialize Prometheus metrics exporter
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("Failed to install Prometheus recorder: {}", e))?;
    info!("✅ Prometheus metrics initialized");

    // Setup database connections
    let db_pool = database::setup_database(&config.database_url).await?;
    info!("✅ PostgreSQL connection established");

    // Run database migrations
    database::run_migrations(&db_pool).await?;
    info!("✅ Database migrations completed");

    // Setup Redis connection
    let redis_client = setup_redis(config).await?;
    info!("✅ Redis connection established");

    // Initialize authentication services
    let jwt_service = JwtService::new()?;
    let api_key_service = ApiKeyService::new()?;
    info!("✅ JWT and API key services initialized");

    // Initialize email service (optional)
    let email_service = initialize_email_service(config);

    // Initialize auth service
    let auth = services::AuthService::new(
        db_pool.clone(),
        config.clone(),
        email_service.clone(),
        jwt_service.clone(),
    );
    info!("✅ Auth service initialized");

    // Initialize blockchain service
    let blockchain_service = services::BlockchainService::new(
        config.solana_rpc_url.clone(),
        "localnet".to_string(),
        config.solana_programs.clone(),
    )?;
    info!("✅ Blockchain service initialized (RPC: {})", config.solana_rpc_url);

    // Initialize wallet service
    let wallet_service = if let Ok(path) = std::env::var("AUTHORITY_WALLET_PATH") {
        info!("Loading authority wallet from: {}", path);
        services::WalletService::with_path(&config.solana_rpc_url, path)
    } else {
        services::WalletService::new(&config.solana_rpc_url)
    };
    initialize_wallet(&wallet_service).await;

    // Initialize WebSocket service
    let websocket_service = services::WebSocketService::new();
    info!("✅ WebSocket service initialized");

    // Initialize cache service
    let cache_service = services::CacheService::new(&config.redis_url).await?;
    info!("✅ Cache service initialized");

    // Initialize health checker
    let health_checker = services::HealthChecker::new(
        db_pool.clone(),
        redis_client.clone(),
        config.solana_rpc_url.clone(),
        email_service.is_some(),
    );
    info!("✅ Health checker initialized");

    // Initialize audit logger
    let audit_logger = services::AuditLogger::new(db_pool.clone());
    info!("✅ Audit logger initialized");

    // Initialize market clearing service
    let market_clearing = services::MarketClearingService::new(
        db_pool.clone(),
        blockchain_service.clone(),
    );
    info!("✅ Market clearing service initialized");

    // Initialize settlement service
    let settlement = services::SettlementService::new(
        db_pool.clone(),
        blockchain_service.clone(),
        config.encryption_secret.clone(),
    );
    info!("✅ Settlement service initialized");

    // Initialize matching engine
    let market_clearing_engine = services::OrderMatchingEngine::new(db_pool.clone())
        .with_websocket(websocket_service.clone())
        .with_settlement(settlement.clone());
    info!("✅ Order matching engine initialized");

    // Initialize futures service
    let futures_service = services::FuturesService::new(db_pool.clone());
    info!("✅ Futures service initialized");

    // Create minimal application state
    let app_state = AppState {
        db: db_pool,
        redis: redis_client,
        config: config.clone(),
        jwt_service,
        api_key_service,
        auth,
        email_service,
        blockchain_service,
        wallet_service,
        websocket_service,
        cache_service,
        health_checker,
        audit_logger,
        market_clearing,
        settlement,
        market_clearing_engine,
        futures_service,
        metrics_handle,
    };

    info!("✅ AppState created successfully with P2P services");
    info!("📊 Ready to receive meter readings at /api/meters/submit-reading");

    Ok(app_state)
}

/// Setup Redis connection.
async fn setup_redis(config: &Config) -> Result<redis::Client> {
    let redis_client = redis::Client::open(config.redis_url.as_str())?;

    // Test Redis connection
    match redis_client.get_multiplexed_async_connection().await {
        Ok(mut conn) => {
            use redis::AsyncCommands;
            match conn.get::<&str, Option<String>>("health_check").await {
                Ok(_) => info!("Redis connection verified"),
                Err(e) => {
                    error!("Redis connection test failed: {}", e);
                    return Err(anyhow::anyhow!("Redis connection test failed: {}", e));
                }
            }
        }
        Err(e) => {
            error!("Failed to establish Redis connection: {}", e);
            return Err(anyhow::anyhow!("Redis connection failed: {}", e));
        }
    }

    Ok(redis_client)
}

/// Initialize email service (optional).
fn initialize_email_service(config: &Config) -> Option<services::EmailService> {
    match services::EmailService::new(&config.email) {
        Ok(service) => {
            info!("Email service initialized");
            Some(service)
        }
        Err(e) => {
            warn!("Email service disabled: {}", e);
            None
        }
    }
}

/// Initialize wallet service and load authority wallet.
async fn initialize_wallet(wallet_service: &services::WalletService) {
    match wallet_service.initialize_authority().await {
        Ok(()) => {
            if let Ok(pubkey) = wallet_service.get_authority_pubkey_string().await {
                info!("🔑 Authority wallet loaded: {}", pubkey);
            }
        }
        Err(e) => {
            warn!(
                "⚠️ Failed to load authority wallet: {}. Token minting will not be available.",
                e
            );
        }
    }
}

/// Spawn background tasks.
pub async fn spawn_background_tasks(app_state: &AppState, _config: &Config) {
    info!("📌 Spawning background tasks...");
    
    // Start the Order Matching Engine
    app_state.market_clearing_engine.start().await;
    info!("✅ Order Matching Engine started");

    // Start Settlement Service Loop
    let settlement = app_state.settlement.clone();
    tokio::spawn(async move {
        info!("🚀 Starting automated settlement processing (interval: 5s)");
        loop {
            match settlement.process_pending_settlements().await {
                Ok(count) => {
                    if count > 0 {
                        info!("✅ Processed {} settlements", count);
                    }
                }
                Err(e) => {
                    error!("❌ Error processing settlements: {}", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });
    info!("✅ Settlement Service started");
}

/// Wait for shutdown signal.
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
