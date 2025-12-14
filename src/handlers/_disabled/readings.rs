//! Meter reading submission handler - Simplified for testing

use axum::{extract::State, Json};
use rust_decimal::prelude::ToPrimitive;
use tracing::{error, info, warn};

use crate::{
    error::{ApiError, Result},
    services::BlockchainService,
    AppState,
};

use super::types::{MeterReadingResponse, SubmitReadingRequest};

/// Submit a new meter reading
/// POST /api/meters/submit-reading
///
/// Simplified handler for testing - bypasses authentication
#[utoipa::path(
    post,
    path = "/api/meters/submit-reading",
    tag = "meters",
    request_body = SubmitReadingRequest,
    responses(
        (status = 200, description = "Meter reading submitted successfully", body = MeterReadingResponse),
        (status = 400, description = "Invalid reading data"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_reading(
    State(state): State<AppState>,
    Json(request): Json<SubmitReadingRequest>,
) -> Result<Json<MeterReadingResponse>> {
    info!(
        "Received meter reading: {} kWh for wallet {:?}",
        request.kwh_amount, request.wallet_address
    );

    // Get wallet address from request (required for simulator)
    let wallet_address = request.wallet_address.clone().ok_or_else(|| {
        ApiError::BadRequest("Wallet address required".to_string())
    })?;

    // Submit reading to database using simplified service call
    let meter_request = crate::services::meter::service::SubmitMeterReadingRequest {
        wallet_address: wallet_address.clone(),
        kwh_amount: request.kwh_amount,
        reading_timestamp: request.reading_timestamp,
        meter_signature: request.meter_signature.clone(),
        meter_serial: request.meter_serial.clone(),
    };

    // Validate the reading
    crate::services::meter::service::MeterService::validate_reading(&meter_request)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Create a mock user ID for testing
    let mock_user_id = uuid::Uuid::nil();

    // Submit reading with minimal verification
    let reading: crate::services::meter::service::MeterReading = state
        .meter_service
        .submit_reading_with_verification(
            mock_user_id,
            meter_request,
            request.meter_id,
            "test_mode",
        )
        .await
        .map_err(|e| {
            error!("Failed to submit meter reading: {}", e);
            ApiError::Internal(format!("Failed to submit reading: {}", e))
        })?;

    info!("Meter reading submitted successfully: {}", reading.id);

    // Convert kWh to f64 for blockchain operations
    let kwh_f64 = request.kwh_amount.to_f64().unwrap_or(0.0);

    // Attempt blockchain minting if amount is positive
    if kwh_f64 > 0.0 {
        info!("Triggering blockchain mint for {} kWh", kwh_f64);

        // Get authority keypair
        match state.wallet_service.get_authority_keypair().await {
            Ok(authority_keypair) => {
                // Parse addresses
                let token_mint_result = BlockchainService::parse_pubkey(&state.config.energy_token_mint);
                let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

                match (token_mint_result, wallet_pubkey_result) {
                    (Ok(token_mint), Ok(wallet_pubkey)) => {
                        // Ensure token account exists
                        match state
                            .blockchain_service
                            .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                            .await
                        {
                            Ok(user_token_account) => {
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
                                        info!("Mint successful: {}", sig_str);
                                        // Mark reading as minted
                                        let _ = state
                                            .meter_service
                                            .mark_as_minted(reading.id, &sig_str)
                                            .await;
                                    }
                                    Err(e) => {
                                        error!("Mint failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to ensure token account exists: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("Invalid token mint or wallet address");
                    }
                }
            }
            Err(e) => {
                warn!("Authority keypair not available - skipping blockchain action: {}", e);
            }
        }
    }

    // Get the final reading state - use the reading we already have and refetch for mint status
    let final_reading: crate::services::meter::service::MeterReading = state
        .meter_service
        .get_reading_by_id(reading.id)
        .await
        .unwrap_or(reading);

    Ok(Json(MeterReadingResponse {
        id: final_reading.id,
        user_id: final_reading
            .user_id
            .ok_or_else(|| ApiError::Internal("Missing user_id".to_string()))?,
        wallet_address: final_reading.wallet_address,
        kwh_amount: final_reading
            .kwh_amount
            .ok_or_else(|| ApiError::Internal("Missing kwh_amount".to_string()))?,
        reading_timestamp: final_reading
            .reading_timestamp
            .ok_or_else(|| ApiError::Internal("Missing reading_timestamp".to_string()))?,
        submitted_at: final_reading
            .submitted_at
            .ok_or_else(|| ApiError::Internal("Missing submitted_at".to_string()))?,
        minted: final_reading.minted.unwrap_or(false),
        mint_tx_signature: final_reading.mint_tx_signature,
    }))
}
