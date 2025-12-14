use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Token balance response
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenBalanceResponse {
    #[schema(example = "100.5")]
    pub balance: f64,
    #[schema(example = "Energy Token")]
    pub token_name: String,
    #[schema(example = "ENT")]
    pub token_symbol: String,
    #[schema(example = 9)]
    pub decimals: u8,
}

/// Token info from blockchain
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenInfoResponse {
    #[schema(example = "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur")]
    pub mint_address: String,
    #[schema(example = "Energy Token")]
    pub name: String,
    #[schema(example = "ENT")]
    pub symbol: String,
    #[schema(example = 9)]
    pub decimals: u8,
    #[schema(example = "1000000.0")]
    pub total_supply: f64,
    #[schema(example = "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7")]
    pub authority: String,
}

/// Get token balance for a wallet address
/// GET /api/tokens/balance/:wallet_address
#[utoipa::path(
    get,
    path = "/api/tokens/balance/{wallet_address}",
    tag = "tokens",
    params(
        ("wallet_address" = String, Path, description = "Wallet address to check balance for")
    ),
    responses(
        (status = 200, description = "Token balance info", body = TokenBalanceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Wallet not found or invalid address"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_token_balance(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(wallet_address): Path<String>,
) -> Result<Json<TokenBalanceResponse>> {
    let _ = user; // Auth check only

    // Get Energy Token Mint
    let mint_str = std::env::var("ENERGY_TOKEN_MINT")
        .map_err(|_| ApiError::Internal("ENERGY_TOKEN_MINT not set".to_string()))?;
    let mint = Pubkey::from_str(&mint_str)
        .map_err(|e| ApiError::Internal(format!("Invalid mint address: {}", e)))?;
    let wallet = Pubkey::from_str(&wallet_address)
        .map_err(|e| ApiError::BadRequest(format!("Invalid wallet address: {}", e)))?;

    // Call BlockchainService to get balance
    let balance_lamports = state
        .blockchain_service
        .get_token_balance(&wallet, &mint)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get token balance: {}", e)))?;

    // Convert to tokens (assuming 9 decimals)
    let balance = balance_lamports as f64 / 1_000_000_000.0;

    Ok(Json(TokenBalanceResponse {
        balance,
        token_name: "Energy Token".to_string(), // TODO: Get from config/chain
        token_symbol: "ENT".to_string(),        // TODO: Get from config/chain
        decimals: 9,                            // TODO: Get from config/chain
    }))
}

/// Get token program info from blockchain
/// GET /api/tokens/info
#[utoipa::path(
    get,
    path = "/api/tokens/info",
    tag = "tokens",
    responses(
        (status = 200, description = "Token program info", body = TokenInfoResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_token_info(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TokenInfoResponse>> {
    // Get Energy Token Mint
    let mint_str = std::env::var("ENERGY_TOKEN_MINT")
        .map_err(|_| ApiError::Internal("ENERGY_TOKEN_MINT not set".to_string()))?;
    let mint = Pubkey::from_str(&mint_str)
        .map_err(|e| ApiError::Internal(format!("Invalid mint address: {}", e)))?;

    // Get Account Data
    let data = state
        .blockchain_service
        .get_account_data(&mint)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get mint account data: {}", e)))?;

    // Parse Mint Data
    let token_info = parse_token_info(&data)?;

    Ok(Json(TokenInfoResponse {
        mint_address: mint_str,
        ..token_info
    }))
}

/// Parse token info from raw bytes
#[allow(dead_code)]
pub fn parse_token_info(data: &[u8]) -> Result<TokenInfoResponse> {
    use solana_program::program_pack::Pack;
    use spl_token::state::Mint;

    let mint = Mint::unpack(data)
        .map_err(|e| ApiError::Internal(format!("Failed to unpack mint data: {}", e)))?;

    Ok(TokenInfoResponse {
        mint_address: "".to_string(), // Not in account data
        name: "Energy Token".to_string(),
        symbol: "ENT".to_string(),
        decimals: mint.decimals,
        total_supply: (mint.supply as f64) / 10f64.powi(mint.decimals as i32),
        authority: mint
            .mint_authority
            .map(|p| p.to_string())
            .unwrap_or_default(),
    })
}
