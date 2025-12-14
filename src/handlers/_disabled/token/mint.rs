use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Mint tokens request (admin only)
#[derive(Debug, Deserialize, ToSchema)]
pub struct MintTokensRequest {
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub recipient_wallet: String,
    #[schema(example = "100.0")]
    pub amount: f64,
}

/// Mint tokens response
#[derive(Debug, Serialize, ToSchema)]
pub struct MintTokensResponse {
    #[schema(example = "4yC...")]
    pub transaction_signature: String,
    #[schema(example = "100.0")]
    pub amount: f64,
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub recipient: String,
}

/// Mint energy tokens (admin only)
/// POST /api/admin/tokens/mint
#[utoipa::path(
    post,
    path = "/api/admin/tokens/mint",
    tag = "admin",
    request_body = MintTokensRequest,
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintTokensResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn mint_tokens(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<MintTokensRequest>,
) -> Result<Json<MintTokensResponse>> {
    // Check admin role
    if user.0.role != "admin" {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    // Perform minting via BlockchainService
    let recipient_pubkey = solana_sdk::pubkey::Pubkey::from_str(&payload.recipient_wallet)
        .map_err(|e| ApiError::BadRequest(format!("Invalid recipient address: {}", e)))?;

    // Convert amount to u64 lamports (assuming 9 decimals for now, or passing float if mint_tokens_direct takes float/u64)
    // mint_tokens_direct takes (user_wallet: &Pubkey, amount: u64).
    // Wait, amount in mint_tokens_direct (Step 560 line 735) takes u64.
    // AND calls token_manager.mint_energy_tokens with amount as f64 / 1_000_000_000.0?
    // Line 764: amount as f64 / 1_000_000_000.0
    // This looks WRONG in blockchain_service.rs if the input is u64 representing tokens?
    // Or if input is Lamports?
    // If I pass 100 (tokens) * 1e9 = 100e9 lamports.
    // 100e9 / 1e9 = 100.0 tokens.
    // So mint_tokens_direct expects Lamports (u64).

    let amount_lamports = (payload.amount * 1_000_000_000.0) as u64;

    let signature = state
        .blockchain_service
        .mint_tokens_direct(&recipient_pubkey, amount_lamports)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to mint tokens: {}", e)))?;

    Ok(Json(MintTokensResponse {
        transaction_signature: signature.to_string(),
        amount: payload.amount,
        recipient: payload.recipient_wallet,
    }))
}
