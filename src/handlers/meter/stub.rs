//! Simplified Meter Stub Handler
//! 
//! This is a minimal meter reading handler that bypasses SQLx compile-time checking
//! by storing readings in memory and triggering blockchain operations directly.

use axum::{
    extract::State,
    Json,
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
                                
                                // Get meter serial for on-chain update
                                let meter_serial = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
                                
                                // Convert kWh to Wh for on-chain storage (u64)
                                let energy_wh = (kwh_f64 * 1000.0) as u64;
                                let reading_timestamp = request.reading_timestamp.timestamp();
                                
                                // Step 1: Update on-chain meter reading via Registry program
                                // Note: Authority must be set as oracle via set_oracle_authority on Registry
                                let registry_update_result = state
                                    .blockchain_service
                                    .update_meter_reading_on_chain(
                                        &authority_keypair,
                                        &meter_serial,
                                        energy_wh,  // energy_generated (for generation readings)
                                        0,          // energy_consumed (0 for generation)
                                        reading_timestamp,
                                    )
                                    .await;
                                
                                match registry_update_result {
                                    Ok(registry_sig) => {
                                        info!("📝 Registry updated on-chain: {}", registry_sig);
                                    }
                                    Err(e) => {
                                        // Log but continue - registry update is optional for now
                                        // This allows graceful degradation if oracle not configured
                                        warn!("⚠️ On-chain registry update failed (continuing): {}", e);
                                    }
                                }
                                
                                // Step 2: Mint tokens (auto-minting)
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
        // Consumption - burn tokens
        let burn_amount = kwh_f64.abs();
        info!("🔥 Triggering token burn for {} kWh consumption", burn_amount);

        // Get authority keypair
        match state.wallet_service.get_authority_keypair().await {
            Ok(authority_keypair) => {
                info!("✅ Authority keypair loaded for burn");
                
                // Parse addresses
                let token_mint_result = BlockchainService::parse_pubkey(&state.config.energy_token_mint);
                let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

                match (token_mint_result, wallet_pubkey_result) {
                    (Ok(token_mint), Ok(wallet_pubkey)) => {
                        info!("✅ Parsed token mint and wallet pubkey for burn");
                        
                        // Get user's token account
                        match state
                            .blockchain_service
                            .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                            .await
                        {
                            Ok(user_token_account) => {
                                info!("✅ Token account exists: {}", user_token_account);
                                
                                // Get meter serial for on-chain update
                                let meter_serial = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
                                
                                // Convert kWh to Wh for on-chain storage (u64)
                                let energy_wh = (burn_amount * 1000.0) as u64;
                                let reading_timestamp = request.reading_timestamp.timestamp();
                                
                                // Step 1: Update on-chain meter reading via Registry program
                                // For consumption, energy_generated=0, energy_consumed=energy_wh
                                let registry_update_result = state
                                    .blockchain_service
                                    .update_meter_reading_on_chain(
                                        &authority_keypair,
                                        &meter_serial,
                                        0,          // energy_generated (0 for consumption)
                                        energy_wh,  // energy_consumed
                                        reading_timestamp,
                                    )
                                    .await;
                                
                                match registry_update_result {
                                    Ok(registry_sig) => {
                                        info!("📝 Registry updated on-chain (consumption): {}", registry_sig);
                                    }
                                    Err(e) => {
                                        warn!("⚠️ On-chain registry update failed (continuing): {}", e);
                                    }
                                }
                                
                                // Step 2: Burn tokens
                                let burn_result = state
                                    .blockchain_service
                                    .burn_energy_tokens(
                                        &authority_keypair,
                                        &user_token_account,
                                        &token_mint,
                                        burn_amount,
                                    )
                                    .await;

                                match burn_result {
                                    Ok(signature) => {
                                        let sig_str = signature.to_string();
                                        info!("🔥 Burn successful! Signature: {}", sig_str);
                                        minted = false; // Not minted, it was burned
                                        mint_tx_signature = Some(sig_str.clone());
                                        message = format!("Consumption of {} kWh recorded. {} tokens burned. TX: {}", burn_amount, burn_amount, sig_str);
                                        
                                        // Broadcast consumption event via WebSocket
                                        let _ = state
                                            .websocket_service
                                            .broadcast_meter_reading_received(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                -burn_amount, // Negative to indicate consumption
                                            )
                                            .await;
                                    }
                                    Err(e) => {
                                        error!("❌ Burn failed: {}", e);
                                        message = format!("Consumption recorded but burn failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to get token account for burn: {}", e);
                                message = format!("Consumption recorded but token account not found: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("❌ Invalid token mint or wallet address for burn");
                        message = "Consumption recorded but invalid addresses".to_string();
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ Authority keypair not available for burn: {}", e);
                message = format!("Consumption recorded but authority wallet not available: {}", e);
            }
        }
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

