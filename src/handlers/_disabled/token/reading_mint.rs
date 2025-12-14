use axum::{extract::State, response::Json};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Mint from meter reading request
#[derive(Debug, Deserialize, ToSchema)]
pub struct MintFromReadingRequest {
    #[schema(example = "123e4567-e89b-12d3-a456-426614174000")]
    pub reading_id: Uuid,
}

/// Mint from meter reading response
#[derive(Debug, Serialize, ToSchema)]
pub struct MintFromReadingResponse {
    pub transaction_signature: String,
    pub amount: f64,
    pub reading_id: Uuid,
    pub status: String,
}

// Local struct for query result to bypass offline checking
#[derive(sqlx::FromRow)]
struct ReadingData {
    id: Uuid,
    #[allow(dead_code)]
    meter_id: String,
    value_kwh: Decimal,
    #[allow(dead_code)]
    timestamp: DateTime<Utc>,
    is_verified: bool,
    minted: Option<bool>,
    #[allow(dead_code)]
    mint_tx_signature: Option<String>,
    user_id: Option<Uuid>,
}

/// Mint tokens from a meter reading
/// POST /api/tokens/mint-from-reading
///
/// Allows users to mint energy tokens based on their submitted meter readings
#[utoipa::path(
    post,
    path = "/api/tokens/mint-from-reading",
    tag = "tokens",
    request_body = MintFromReadingRequest,
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintFromReadingResponse),
        (status = 400, description = "Invalid reading or not eligible for minting"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Reading not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn mint_from_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<MintFromReadingRequest>,
) -> Result<Json<MintFromReadingResponse>> {
    // 1. Fetch reading and verify ownership
    let reading = sqlx::query_as::<_, ReadingData>(
        "SELECT r.id, r.meter_id, r.value_kwh, r.timestamp, r.is_verified, 
                r.minted, r.mint_tx_signature, m.user_id 
         FROM meter_readings r
         JOIN meters m ON r.meter_id = m.id
         WHERE r.id = $1",
    )
    .bind(request.reading_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let reading = reading.ok_or_else(|| ApiError::NotFound("Reading not found".to_string()))?;

    // Verify user owns the meter
    if reading.user_id != Some(user.sub) {
        return Err(ApiError::Forbidden(
            "You do not own the meter for this reading".to_string(),
        ));
    }

    // 2. Validate minting eligibility
    if !reading.is_verified {
        return Err(ApiError::BadRequest("Reading is not verified".to_string()));
    }

    // Check if already minted (using bool 'minted' column)
    if reading.minted.unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "Tokens already minted for this reading".to_string(),
        ));
    }

    // 3. Get user wallet
    let user_wallet_str =
        sqlx::query_scalar::<_, String>("SELECT wallet_address FROM users WHERE id = $1")
            .bind(user.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::BadRequest("User has no wallet address".to_string()))?;

    let user_wallet = solana_sdk::pubkey::Pubkey::from_str(&user_wallet_str)
        .map_err(|e| ApiError::Internal(format!("Invalid wallet in DB: {}", e)))?;

    // 4. Calculate token amount (1 kWh = 1 Token)
    let token_amount_val = reading.value_kwh; // Decimal
    let token_amount = token_amount_val.to_f64().unwrap_or(0.0);
    let amount_lamports = (token_amount * 1_000_000_000.0) as u64;

    // 5. Mint tokens
    let signature = state
        .blockchain_service
        .mint_tokens_direct(&user_wallet, amount_lamports)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to mint tokens: {}", e)))?
        .to_string();

    // 6. Update reading status
    sqlx::query(
        "UPDATE meter_readings 
         SET minted = true, 
             mint_tx_signature = $1,
             updated_at = NOW()
         WHERE id = $2",
    )
    .bind(&signature)
    .bind(reading.id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to update reading status: {}", e)))?;

    Ok(Json(MintFromReadingResponse {
        transaction_signature: signature,
        amount: token_amount,
        reading_id: reading.id,
        status: "success".to_string(),
    }))
}
