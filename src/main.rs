//! GridTokenX API Gateway
//!
//! Main entry point for the P2P Energy Trading System API Gateway.
//! This is a thin entry point that delegates to modular components.

use anyhow::Result;
use std::net::SocketAddr;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

// Use the library crate's public exports
use api_gateway::{config::Config, router, startup, utils};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting GridTokenX API Gateway");

    // Validate secrets and security configuration
    utils::validate_secrets()?;

    // Initialize Prometheus metrics exporter
    let prometheus_builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    if let Err(e) = prometheus_builder.install() {
        error!("Failed to install Prometheus exporter: {}", e);
        warn!("Continuing without metrics export");
    } else {
        info!("Prometheus metrics exporter initialized");
    }

    // Load configuration
    let config = Config::from_env()?;
    info!(
        "Loaded configuration for environment: {}",
        config.environment
    );

    // Initialize all services and create app state
    let app_state = startup::initialize_app(&config).await?;

    // Spawn background tasks
    startup::spawn_background_tasks(&app_state, &config).await;

    // Build router with all routes
    let app = router::build_router(app_state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("Starting API Gateway server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Setup graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(startup::shutdown_signal())
        .await?;

    Ok(())
}
