use axum::{
    extract::{Path, State},
    response::Json,
};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::{debug, error, info};

use super::types::BlockchainUserAccount;
use super::utils::parse_user_account;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Get blockchain user account by wallet address
/// GET /api/blockchain/users/:wallet_address
#[utoipa::path(
    get,
    path = "/api/blockchain/users/{wallet_address}",
    tag = "registry",
    security(("bearer_auth" = [])),
    params(
        ("wallet_address" = String, Path, description = "Solana wallet address (base58)")
    ),
    responses(
        (status = 200, description = "Blockchain user account information", body = BlockchainUserAccount),
        (status = 400, description = "Invalid wallet address"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User account not found on blockchain"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_blockchain_user(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(wallet_address): Path<String>,
) -> Result<Json<BlockchainUserAccount>> {
    info!(
        "Fetching blockchain user account: {} by user: {}",
        wallet_address, user.0.sub
    );

    // Parse the wallet address
    let pubkey = Pubkey::from_str(&wallet_address).map_err(|e| {
        error!("Invalid wallet address '{}': {}", wallet_address, e);
        ApiError::BadRequest(format!("Invalid wallet address: {}", e))
    })?;

    // Derive the user account PDA (Program Derived Address)
    // User account PDA seeds: ["user", user_authority.key()]
    let registry_program_id = state
        .blockchain_service
        .registry_program_id()
        .map_err(|e| {
            error!("Failed to parse registry program ID: {}", e);
            ApiError::Internal(format!("Invalid program ID: {}", e))
        })?;

    let (user_pda, _bump) =
        Pubkey::find_program_address(&[b"user", pubkey.as_ref()], &registry_program_id);

    debug!("User PDA: {}", user_pda);

    // Check if the account exists
    let account_exists = state
        .blockchain_service
        .account_exists(&user_pda)
        .await
        .map_err(|e| {
            error!("Failed to check if account exists: {}", e);
            ApiError::Internal(format!("Blockchain error: {}", e))
        })?;

    if !account_exists {
        return Err(ApiError::NotFound(format!(
            "User account not found for wallet: {}",
            wallet_address
        )));
    }

    // Get the account data
    let account_data = state
        .blockchain_service
        .get_account_data(&user_pda)
        .await
        .map_err(|e| {
            error!("Failed to fetch account data: {}", e);
            ApiError::Internal(format!("Failed to fetch account: {}", e))
        })?;

    // Deserialize the account data
    // Anchor accounts have an 8-byte discriminator at the start
    if account_data.len() < 8 {
        return Err(ApiError::Internal("Invalid account data".to_string()));
    }

    // Parse the account data (skip 8-byte discriminator)
    let user_account = parse_user_account(&account_data[8..]).map_err(|e| {
        error!("Failed to parse user account: {}", e);
        ApiError::Internal(format!("Failed to parse account data: {}", e))
    })?;

    info!("Successfully fetched user account: {}", wallet_address);
    Ok(Json(user_account))
}
