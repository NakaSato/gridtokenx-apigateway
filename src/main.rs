use std::net::SocketAddr;

use anyhow::Result;
use axum::{routing::{get, post}, Router, middleware::from_fn_with_state};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer, timeout::TimeoutLayer};
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;

mod config;
mod database;
mod handlers;
mod middleware;
mod models;
mod services;
mod utils;
mod error;
mod auth;
mod openapi;

use config::Config;
use handlers::{health, auth as auth_handlers, user_management, blockchain, trading, meters, wallet_auth, registry, oracle, governance, token, erc, blockchain_test, audit, admin, epochs};
use auth::{jwt::JwtService, jwt::ApiKeyService};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub timescale_db: Option<sqlx::PgPool>,
    pub redis: redis::Client,
    pub config: Config,
    pub jwt_service: JwtService,
    pub api_key_service: ApiKeyService,
    pub email_service: Option<services::EmailService>,
    pub blockchain_service: services::BlockchainService,
    pub wallet_service: services::WalletService,
    pub meter_service: services::MeterService,
    pub meter_verification_service: services::MeterVerificationService,
    pub erc_service: services::ErcService,
    pub order_matching_engine: services::OrderMatchingEngine,
    pub market_clearing_engine: services::MarketClearingEngine,
    pub market_clearing_service: services::MarketClearingService,
    pub websocket_service: services::WebSocketService,
    pub health_checker: services::HealthChecker,
    pub audit_logger: services::AuditLogger,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=debug,tower_http=debug".into())
        )
        .init();

    // Validate secrets and security configuration before proceeding
    utils::validate_secrets()?;

    // Initialize Prometheus metrics exporter
    let prometheus_builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    prometheus_builder
        .install()
        .expect("Failed to install Prometheus exporter");
    info!("Prometheus metrics exporter initialized");

    // Load configuration
    let config = Config::from_env()?;
    info!("Loaded configuration for environment: {}", config.environment);

    // Setup database connections
    let db_pool = database::setup_database(&config.database_url).await?;
    info!("PostgreSQL connection established");

    let timescale_pool = database::setup_timescale_database(&config.influxdb_url).await?;
    if timescale_pool.is_some() {
        info!("TimescaleDB connection established");
    }

    // Run database migrations (PostgreSQL only - TimescaleDB has its own schema)
    database::run_migrations(&db_pool).await?;
    info!("Database migrations completed successfully");

    // Setup Redis connection with authentication support
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    
    // Test Redis connection and validate authentication
    match redis_client.get_multiplexed_async_connection().await {
        Ok(mut conn) => {
            // Test basic Redis operation
            use redis::AsyncCommands;
            match conn.get::<&str, Option<String>>("health_check").await {
                Ok(_) => {
                    let auth_status = if config.redis_url.contains("@") {
                        "✅ Redis connection established (authenticated)"
                    } else {
                        "⚠️  Redis connection established (no authentication - consider adding password)"
                    };
                    info!("{}", auth_status);
                    
                    // Additional security warning for production
                    if config.environment == "production" && !config.redis_url.contains("@") {
                        warn!("🚨 SECURITY WARNING: Redis connection in production is not authenticated!");
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
            
            // Provide helpful error message for authentication issues
            if e.to_string().contains("NOAUTH") {
                error!("Redis authentication failed. Please check your REDIS_URL format:");
                error!("  Correct format: redis://:password@host:port");
                error!("  Current URL: {}", config.redis_url);
            } else if e.to_string().contains("Connection refused") {
                error!("Redis server is not running or not accessible at: {}", config.redis_url);
            }
            
            return Err(anyhow::anyhow!("Redis connection failed: {}", e));
        }
    }

    // Initialize authentication services
    let jwt_service = JwtService::new()?;
    let api_key_service = ApiKeyService::new()?;
    info!("Authentication services initialized");

    // Initialize email service (optional - may fail if SMTP not configured)
    let email_service = match services::EmailService::new(&config.email) {
        Ok(service) => {
            info!("Email service initialized successfully");
            Some(service)
        }
        Err(e) => {
            tracing::warn!("Email service initialization failed: {}. Email verification will be disabled.", e);
            None
        }
    };

    // Initialize blockchain service
    let blockchain_service = services::BlockchainService::new(
        config.solana_rpc_url.clone(),
        "localnet".to_string(),
    )?;
    info!("Blockchain service initialized");

    // Initialize wallet service (Phase 4)
    let wallet_service = services::WalletService::new(&config.solana_rpc_url);
    
    // Try to load authority wallet
    match wallet_service.initialize_authority().await {
        Ok(()) => {
            let pubkey = wallet_service.get_authority_pubkey_string().await?;
            info!("Authority wallet loaded: {}", pubkey);
        }
        Err(e) => {
            tracing::warn!("Failed to load authority wallet: {}. Token minting will not be available.", e);
        }
    }

    // Initialize meter service (Phase 4)
    let meter_service = services::MeterService::new(db_pool.clone());
    info!("Meter service initialized");

    // Initialize meter verification service (Priority 0 - Security)
    let meter_verification_service = services::MeterVerificationService::new(db_pool.clone());
    info!("Meter verification service initialized");

    // Initialize ERC service (Phase 4)
    let erc_service = services::ErcService::new(db_pool.clone());
    info!("ERC service initialized");

    // Initialize market clearing service (Phase 5) for epoch-based order management
    let market_clearing_service = services::MarketClearingService::new(db_pool.clone());
    info!("✅ Market clearing service initialized for epoch management");

    // Initialize WebSocket service for real-time market updates
    let websocket_service = services::WebSocketService::new();
    info!("✅ WebSocket service initialized for real-time updates");

    // Initialize automated order matching engine
    let order_matching_engine = services::OrderMatchingEngine::new(db_pool.clone())
        .with_websocket(websocket_service.clone());
    info!("Order matching engine initialized");

    // Initialize market clearing engine for P2P energy trading
    let market_clearing_engine = services::MarketClearingEngine::new(
        db_pool.clone(),
        redis_client.clone()
    )
    .with_websocket(websocket_service.clone());
    info!("✅ Market clearing engine initialized with WebSocket support");
    
    // Load active orders into order book
    match market_clearing_engine.load_order_book().await {
        Ok(count) => info!("Loaded {} active orders into order book", count),
        Err(e) => warn!("Failed to load order book: {}", e),
    }

    // Start the background matching service
    order_matching_engine.start().await;
    info!("✅ Automated order matching engine started");

    // Initialize epoch scheduler with 15-minute intervals
    let epoch_scheduler = std::sync::Arc::new(
        services::EpochScheduler::new(
            db_pool.clone(),
            services::EpochConfig::default(),
        )
    );
    info!("✅ Epoch scheduler initialized (15-minute intervals)");

    // Start epoch scheduler in background
    match epoch_scheduler.start().await {
        Ok(_) => info!("🚀 Epoch scheduler started successfully"),
        Err(e) => warn!("⚠️  Failed to start epoch scheduler: {}", e),
    }

    // Initialize health checker with dependencies
    let health_checker = services::HealthChecker::new(
        db_pool.clone(),
        redis_client.clone(),
        config.solana_rpc_url.clone(),
    );
    info!("Health checker initialized");

    // Initialize audit logger for security event tracking
    let audit_logger = services::AuditLogger::new(db_pool.clone());
    info!("✅ Audit logger initialized for security event tracking");

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
        websocket_service: websocket_service.clone(),
        health_checker,
        audit_logger,
    };

    // Build API routes
    
    // Public API routes
    let public_routes = Router::new()
        // Health check routes
        .route("/health", get(health::health_check))
        .route("/metrics", get(|| async { "# Metrics endpoint\n" }))
        
        // Public API routes
        .route("/api/auth/login", post(auth_handlers::login))
        .route("/api/auth/register", post(user_management::register))
        .route("/api/auth/verify-email", get(handlers::email_verification::verify_email))
        .route("/api/auth/resend-verification", post(handlers::email_verification::resend_verification))
        .route("/api/auth/wallet/login", post(wallet_auth::login_with_wallet))
        .route("/api/auth/wallet/register", post(wallet_auth::register_with_wallet))
        
        // Public market endpoints
        .route("/api/market/epoch", get(epochs::get_current_epoch))
        .route("/api/market/epoch/status", get(epochs::get_epoch_status))
        .route("/api/market/orderbook", get(handlers::energy_trading_simple::get_orderbook))
        .route("/api/market/stats", get(handlers::energy_trading_simple::get_market_stats))
        
        // WebSocket endpoints
        .route("/api/market/ws", get(handlers::websocket::market_websocket_handler))
        .route("/ws", get(handlers::websocket::websocket_handler))
        
        // Swagger UI
        .merge(SwaggerUi::new("/api/docs")
            .url("/api/docs/openapi.json", openapi::ApiDoc::openapi()));
    
    // V1 routes (for backward compatibility)
    let v1_routes = public_routes.clone();
    
    // Versioned public routes
    let versioned_public_routes = Router::new()
        .nest("/api/v1", v1_routes)
        .fallback(get(|| async { 
            axum::Json(serde_json::json!({
                "error": "Version not specified. Use /api/v1/",
                "supported_versions": ["v1"]
            }))
        }));
    
    // Protected routes (authentication required)
    let protected_routes = Router::new()
        // Protected auth routes
        .route("/api/auth/profile", get(auth_handlers::get_profile))
        .route("/api/auth/profile/update", post(auth_handlers::update_profile))
        .route("/api/auth/password", post(auth_handlers::change_password))
        
        // user management routes
        .nest("/api/user", Router::new()
            .route("/wallet", post(user_management::update_wallet_address))
            .route("/wallet", axum::routing::delete(user_management::remove_wallet_address))
            .route("/activity", get(user_management::get_user_activity))
        )
        
        // Admin-only user management routes
        .nest("/api/users", Router::new()
            .route("/{id}", get(auth_handlers::get_user))
            .route("/{id}", axum::routing::put(user_management::admin_update_user))
            .route("/{id}/deactivate", post(user_management::admin_deactivate_user))
            .route("/{id}/reactivate", post(user_management::admin_reactivate_user))
            .route("/{id}/activity", get(user_management::get_user_activity))
            .route("/", get(auth_handlers::list_users))
        )
        
        // Blockchain interaction routes
        .nest("/api/blockchain", Router::new()
            .route("/transactions", post(blockchain::submit_transaction))
            .route("/transactions", get(blockchain::get_transaction_history))
            .route("/transactions/{signature}", get(blockchain::get_transaction_status))
            .route("/programs/{name}", post(blockchain::interact_with_program))
            .route("/accounts/{address}", get(blockchain::get_account_info))
            .route("/network", get(blockchain::get_network_status))
            // Registry program endpoints
            .route("/users/{wallet_address}", get(registry::get_blockchain_user))
        )
        
        // Blockchain testing routes
        .nest("/api/test", Router::new()
            .route("/transactions", post(blockchain_test::create_test_transaction))
            .route("/transactions/{signature}", get(blockchain_test::get_test_transaction_status))
            .route("/statistics", get(blockchain_test::get_test_statistics))
        )
        
        // Admin-only routes
        .nest("/api/admin", Router::new()
            .route("/users/{id}/update-role", post(registry::update_user_role))
            // Governance admin routes
            .route("/governance/emergency-pause", post(governance::emergency_pause))
            .route("/governance/unpause", post(governance::emergency_unpause))
            // Token admin routes
            .route("/tokens/mint", post(token::mint_tokens))
            // Trading admin routes
            .route("/trading/match-orders", post(trading::match_blockchain_orders))
            // Market admin routes
            .route("/market/health", get(admin::get_market_health))
            .route("/market/analytics", get(admin::get_trading_analytics))
            .route("/market/control", post(admin::market_control))
            // Audit log routes
            .route("/audit/user/{user_id}", get(audit::get_user_audit_logs))
            .route("/audit/type/{event_type}", get(audit::get_audit_logs_by_type))
            .route("/audit/security", get(audit::get_security_events))
            // Epoch management
            .route("/epochs", get(epochs::list_all_epochs))
            .route("/epochs/{epoch_id}/stats", get(epochs::get_epoch_stats))
            .route("/epochs/{epoch_id}/trigger", post(epochs::trigger_manual_clearing))
        )
        
        // Oracle routes
        .nest("/api/oracle", Router::new()
            .route("/prices", post(oracle::submit_price))
            .route("/prices/current", get(oracle::get_current_prices))
            .route("/data", get(oracle::get_oracle_data))
        )
        
        // Governance routes
        .nest("/api/governance", Router::new()
            .route("/status", get(governance::get_governance_status))
        )
        
        // P2P Energy Trading routes (authenticated users)
        .nest("/api/market", Router::new()
            .route("/depth", get(handlers::market_data::get_order_book_depth))
            .route("/depth-chart", get(handlers::market_data::get_market_depth_chart))
            .route("/clearing-price", get(handlers::market_data::get_clearing_price))
            .route("/trades/my-history", get(handlers::market_data::get_my_trade_history))
        )
        
        // Simplified Energy Trading routes
        .nest("/api/trading", Router::new()
            .route("/orders", post(handlers::energy_trading_simple::create_order))
            .route("/orders", get(handlers::energy_trading_simple::list_orders))
        )
        
        // Analytics routes
        .route("/api/analytics/market", get(handlers::analytics::get_market_analytics))
        .route("/api/analytics/my-stats", get(handlers::analytics::get_user_trading_stats))
        
        // Token routes
        .nest("/api/tokens", Router::new()
            .route("/balance/{wallet_address}", get(token::get_token_balance))
            .route("/info", get(token::get_token_info))
            .route("/mint-from-reading", post(token::mint_from_reading))
        )
        
        // Energy meter routes - Phase 4
        .nest("/api/meters", Router::new()
            .route("/verify", post(handlers::meter_verification::verify_meter_handler))
            .route("/registered", get(handlers::meter_verification::get_registered_meters_handler))
            .route("/submit-reading", post(meters::submit_reading))
            .route("/my-readings", get(meters::get_my_readings))
            .route("/readings/{wallet_address}", get(meters::get_readings_by_wallet))
            .route("/stats", get(meters::get_user_stats))
        )
        
        // Admin meter routes - Phase 4
        .nest("/api/admin/meters", Router::new()
            .route("/unminted", get(meters::get_unminted_readings))
            .route("/mint-from-reading", post(meters::mint_from_reading))
        )
        
        // Energy Renewable Certificate (ERC) routes - Phase 4
        .nest("/api/erc", Router::new()
            .route("/issue", post(erc::issue_certificate))
            .route("/my-certificates", get(erc::get_my_certificates))
            .route("/my-stats", get(erc::get_my_certificate_stats))
            .route("/{certificate_id}", get(erc::get_certificate))
            .route("/{certificate_id}/retire", post(erc::retire_certificate))
            .route("/wallet/{wallet_address}", get(erc::get_certificates_by_wallet))
        )
        .layer(from_fn_with_state(app_state.clone(), auth::middleware::auth_middleware))
        .layer(axum::middleware::from_fn(middleware::auth_logger_middleware));

    // V1 protected routes (for backward compatibility)
    let v1_protected_routes = protected_routes.clone();
    
    // Versioned protected routes
    let versioned_protected_routes = Router::new()
        .nest("/api/v1", v1_protected_routes);

    // Combine all routes
    let app = versioned_public_routes
        .merge(versioned_protected_routes)
        .layer(
            ServiceBuilder::new()
                .layer(axum::middleware::from_fn(middleware::versioning_middleware))
                .layer(axum::middleware::from_fn(middleware::version_check_middleware))
                .layer(axum::middleware::from_fn(middleware::add_security_headers))
                .layer(axum::middleware::from_fn(middleware::metrics_middleware))
                .layer(axum::middleware::from_fn(middleware::active_requests_middleware))
                .layer(axum::middleware::from_fn(middleware::request_logger_middleware))
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
                .layer(CorsLayer::permissive())
        )
        .with_state(app_state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("Starting API Gateway server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    // Setup graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    
    Ok(())
}

/// Wait for SIGTERM or SIGINT signal for graceful shutdown
async fn shutdown_signal() {
    use tokio::signal;
    
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal, shutting down gracefully");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM signal, shutting down gracefully");
        },
    }
}
