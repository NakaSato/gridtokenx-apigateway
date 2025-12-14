//! Simplified Meter Stub Handler
//! 
//! This is a minimal meter reading handler that bypasses SQLx compile-time checking
//! by storing readings in memory and triggering blockchain operations directly.

use axum::{
    extract::State,
    Json,
    routing::post,
    Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    error::{ApiError, Result},
    services::BlockchainService,
    AppState,
};

/// Request to submit a meter reading
#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitReadingRequest {
    pub wallet_address: Option<String>,
    pub kwh_amount: Decimal,
    pub reading_timestamp: DateTime<Utc>,
    pub meter_signature: Option<String>,
    pub meter_serial: Option<String>,
    pub meter_id: Option<Uuid>,
}

/// Response after submitting a reading
#[derive(Debug, Serialize)]
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub wallet_address: String,
    pub kwh_amount: Decimal,
    pub reading_timestamp: DateTime<Utc>,
    pub submitted_at: DateTime<Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
    pub message: String,
}

/// Submit a new meter reading (simplified, no database)
/// POST /submit-reading
pub async fn submit_reading(
    State(state): State<AppState>,
    Json(request): Json<SubmitReadingRequest>,
) -> Result<Json<MeterReadingResponse>> {
    info!(
        "📊 Received meter reading: {} kWh for wallet {:?}",
        request.kwh_amount, request.wallet_address
    );

    // Get wallet address from request (required for simulator)
    let wallet_address = request.wallet_address.clone().ok_or_else(|| {
        ApiError::BadRequest("Wallet address required".to_string())
    })?;

    // Generate a reading ID (in real implementation this would be from database)
    let reading_id = Uuid::new_v4();
    let submitted_at = Utc::now();

    // Validate the reading
    let kwh_f64 = request.kwh_amount.to_f64().unwrap_or(0.0);
    
    if kwh_f64.abs() > 100.0 {
        return Err(ApiError::BadRequest("kWh amount exceeds maximum (100 kWh)".to_string()));
    }

    info!("✅ Reading validated. ID: {}, Amount: {} kWh", reading_id, kwh_f64);

    // Track minting result
    let mut minted = false;
    let mut mint_tx_signature: Option<String> = None;
    let mut message = "Reading received".to_string();

    // Attempt blockchain minting if amount is positive
    if kwh_f64 > 0.0 {
        info!("🔗 Triggering blockchain mint for {} kWh", kwh_f64);

        // Get authority keypair
        match state.wallet_service.get_authority_keypair().await {
            Ok(authority_keypair) => {
                info!("✅ Authority keypair loaded");
                
                // Parse addresses
                let token_mint_result = BlockchainService::parse_pubkey(&state.config.energy_token_mint);
                let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

                match (token_mint_result, wallet_pubkey_result) {
                    (Ok(token_mint), Ok(wallet_pubkey)) => {
                        info!("✅ Parsed token mint and wallet pubkey");
                        
                        // Ensure token account exists
                        match state
                            .blockchain_service
                            .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                            .await
                        {
                            Ok(user_token_account) => {
                                info!("✅ Token account exists: {}", user_token_account);
                                
                                // Mint tokens
                                let mint_result = state
                                    .blockchain_service
                                    .mint_energy_tokens(
                                        &authority_keypair,
                                        &user_token_account,
                                        &wallet_pubkey,
                                        &token_mint,
                                        kwh_f64,
                                    )
                                    .await;

                                match mint_result {
                                    Ok(signature) => {
                                        let sig_str = signature.to_string();
                                        info!("🎉 Mint successful! Signature: {}", sig_str);
                                        minted = true;
                                        mint_tx_signature = Some(sig_str.clone());
                                        message = format!("Reading received and {} kWh minted. TX: {}", kwh_f64, sig_str);
                                        
                                        // Broadcast meter reading received via WebSocket
                                        let _ = state
                                            .websocket_service
                                            .broadcast_meter_reading_received(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                kwh_f64,
                                            )
                                            .await;
                                        
                                        // Broadcast tokens minted via WebSocket
                                        let tokens_minted = (kwh_f64 * 1_000_000_000.0) as u64;
                                        let _ = state
                                            .websocket_service
                                            .broadcast_tokens_minted(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                kwh_f64,
                                                tokens_minted,
                                                &sig_str,
                                            )
                                            .await;
                                    }
                                    Err(e) => {
                                        error!("❌ Mint failed: {}", e);
                                        message = format!("Reading received but minting failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to ensure token account exists: {}", e);
                                message = format!("Reading received but token account creation failed: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("❌ Invalid token mint or wallet address");
                        message = "Reading received but invalid addresses".to_string();
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ Authority keypair not available - skipping blockchain action: {}", e);
                message = format!("Reading received but authority wallet not available: {}", e);
            }
        }
    } else if kwh_f64 < 0.0 {
        // Consumption (burn) - simplified message
        message = format!("Consumption of {} kWh recorded (burn not implemented in stub)", kwh_f64.abs());
    }

    Ok(Json(MeterReadingResponse {
        id: reading_id,
        wallet_address,
        kwh_amount: request.kwh_amount,
        reading_timestamp: request.reading_timestamp,
        submitted_at,
        minted,
        mint_tx_signature,
        message,
    }))
}

/// Health check for meter service
pub async fn meter_health() -> &'static str {
    "Meter stub service is running"
}

/// Build the meter stub routes
pub fn meter_routes() -> Router<AppState> {
    Router::new()
        .route("/submit-reading", post(submit_reading))
        .route("/health", axum::routing::get(meter_health))
}
