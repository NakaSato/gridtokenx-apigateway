//! GridTokenX API Gateway
//!
//! testing Simulator → Gateway → Anchor flow.

use anyhow::Result;
use std::net::SocketAddr;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use api_gateway::{
    config::Config,
    router,
    startup,
    utils,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("🚀 Starting GridTokenX API Gateway");
    info!("📊 Full-featured build with all endpoints enabled");

    // Validate secrets and security configuration
    if let Err(e) = utils::validate_secrets() {
        warn!("⚠️ Secret validation warning: {}", e);
    }

    // Load configuration
    let config = Config::from_env()?;
    info!(
        "Loaded configuration for environment: {}",
        config.environment
    );

    // Initialize all services and create app state
    let app_state = startup::initialize_app(&config).await?;

    // Spawn background tasks (minimal - mostly no-ops)
    startup::spawn_background_tasks(&app_state, &config).await;

    // Build minimal API router
    let app = router::build_router(app_state)
        .layer(tower_http::compression::CompressionLayer::new());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("✅ Starting API Gateway server on {}", addr);
    info!("📍 Health check: http://localhost:{}/health", config.port);
    info!("📍 Meter endpoint: http://localhost:{}/api/meters/submit-reading", config.port);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Setup graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(startup::shutdown_signal())
        .await?;

    Ok(())
}
