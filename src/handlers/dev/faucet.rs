use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use crate::error::{ApiError, Result};
use crate::AppState;

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct FaucetRequest {
    pub wallet_address: String,
    pub amount_sol: Option<f64>,
    pub mint_tokens_kwh: Option<f64>,
    pub deposit_fiat: Option<f64>,
    pub promote_to_role: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FaucetResponse {
    pub success: bool,
    pub message: String,
    pub sol_tx_signature: Option<String>,
    pub token_tx_signature: Option<String>,
}

/// Request funds from the developer faucet
/// POST /api/dev/faucet
#[utoipa::path(
    post,
    path = "/api/dev/faucet",
    tag = "dev",
    request_body = FaucetRequest,
    responses(
        (status = 200, description = "Funds requested successfully", body = FaucetResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn request_faucet(
    State(state): State<AppState>,
    Json(payload): Json<FaucetRequest>,
) -> Result<Json<FaucetResponse>> {
    tracing::info!("Faucet request for wallet: {}", payload.wallet_address);

    let wallet_pubkey = Pubkey::from_str(&payload.wallet_address)
        .map_err(|_| ApiError::BadRequest("Invalid wallet address".to_string()))?;

    let mut sol_sig = None;
    let mut token_sig = None;
    let mut messages = Vec::new();

    // 1. Airdrop SOL
    if let Some(amount) = payload.amount_sol {
        if amount > 0.0 {
            match state
                .wallet_service
                .request_airdrop(&wallet_pubkey, amount)
                .await
            {
                Ok(sig) => {
                    sol_sig = Some(sig.to_string());
                    messages.push(format!("Airdropped {} SOL", amount));
                }
                Err(e) => {
                    tracing::error!("Faucet Airdrop failed: {}", e);
                    // Don't fail the whole request, but note it? 
                    // Or fail? Let's fail if requested explicitly.
                    return Err(ApiError::Internal(format!("Failed to airdrop SOL: {}", e)));
                }
            }
        }
    }

    // 2. Mint Tokens
    if let Some(kwh) = payload.mint_tokens_kwh {
        if kwh > 0.0 {
            // Calculate atomic amount: kwh * 10^9
            let amount_atomic = (kwh * 1_000_000_000.0) as u64;
            
            match state
                .blockchain_service
                .mint_tokens_direct(&wallet_pubkey, amount_atomic as f64)
                .await
            {
                Ok(sig) => {
                    token_sig = Some(sig.to_string());
                    messages.push(format!("Minted {} kWh tokens", kwh));
                }
                Err(e) => {
                    tracing::error!("Faucet Minting failed: {}", e);
                     return Err(ApiError::Internal(format!("Failed to mint tokens: {}", e)));
                }
            }
        }
    }

    // 3. Deposit Fiat (Cash) - Only for dev testing
    if let Some(fiat_amount) = payload.deposit_fiat {
        if fiat_amount > 0.0 {
            // Find user by wallet address in user_wallets or users table
            let user_info = sqlx::query!(
                r#"
                SELECT user_id FROM user_wallets WHERE wallet_address = $1
                UNION
                SELECT id as user_id FROM users WHERE wallet_address = $1
                LIMIT 1
                "#,
                payload.wallet_address
            )
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

            if let Some(u) = user_info {
                use rust_decimal::prelude::FromPrimitive;
                let amount_dec = rust_decimal::Decimal::from_f64(fiat_amount)
                    .ok_or(ApiError::BadRequest("Invalid amount".to_string()))?;

                sqlx::query!(
                    "UPDATE users SET balance = balance + $1 WHERE id = $2",
                    amount_dec,
                    u.user_id
                )
                .execute(&state.db)
                .await
                .map_err(|e| ApiError::Internal(format!("Failed to deposit funds: {}", e)))?;
                
                messages.push(format!("Deposited {} THB", fiat_amount));
            } else {
                 messages.push(format!("Wallet {} not linked to user, skipped fiat deposit", payload.wallet_address));
            }
        }
    }

    // 4. Promote to Role - Only for dev testing
    if let Some(role) = &payload.promote_to_role {
        // Find user by wallet address
        let user_info = sqlx::query!(
             r#"
            SELECT user_id FROM user_wallets WHERE wallet_address = $1
            UNION
            SELECT id as user_id FROM users WHERE wallet_address = $1
            LIMIT 1
            "#,
            payload.wallet_address
        )
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

        if let Some(u) = user_info {
             sqlx::query!(
                "UPDATE users SET role = $1::text::user_role WHERE id = $2",
                role,
                u.user_id
            )
            .execute(&state.db)
            .await
             .map_err(|e| ApiError::Internal(format!("Failed to update role: {}", e)))?;
             
             messages.push(format!("Promoted user to role: {}", role));
        } else {
             messages.push(format!("Wallet {} not linked to user, skipped role promotion", payload.wallet_address));
        }
    }

    Ok(Json(FaucetResponse {
        success: true,
        message: if messages.is_empty() {
            "No actions requested".to_string()
        } else {
            messages.join(", ")
        },
        sol_tx_signature: sol_sig,
        token_tx_signature: token_sig,
    }))
}
