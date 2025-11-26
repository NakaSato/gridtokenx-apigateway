use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::{error, info};
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};

/// Token balance response
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenBalanceResponse {
    pub wallet_address: String,
    pub token_balance: String,
    pub token_balance_raw: u64,
    pub balance_sol: f64,
    pub decimals: u8,
    pub token_mint: String,
    pub token_account: String,
}

/// Token info from blockchain
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenInfoResponse {
    pub authority: String,
    pub mint: String,
    pub total_supply: u64,
    pub created_at: i64,
}

/// Get token balance for a wallet address
/// GET /api/tokens/balance/:wallet_address
#[utoipa::path(
    get,
    path = "/api/tokens/balance/{wallet_address}",
    tag = "tokens",
    security(("bearer_auth" = [])),
    params(
        ("wallet_address" = String, Path, description = "Solana wallet address (base58)")
    ),
    responses(
        (status = 200, description = "Token balance information", body = TokenBalanceResponse),
        (status = 400, description = "Invalid wallet address"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_token_balance(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(wallet_address): Path<String>,
) -> Result<Json<TokenBalanceResponse>> {
    info!(
        "Fetching token balance for wallet: {} by user: {}",
        wallet_address, user.0.sub
    );

    // Parse the wallet address
    let wallet_pubkey = Pubkey::from_str(&wallet_address).map_err(|e| {
        error!("Invalid wallet address '{}': {}", wallet_address, e);
        ApiError::BadRequest(format!("Invalid wallet address: {}", e))
    })?;

    // Get the wallet's SOL balance
    let sol_balance = state
        .blockchain_service
        .get_balance_sol(&wallet_pubkey)
        .await
        .map_err(|e| {
            error!("Failed to fetch SOL balance: {}", e);
            ApiError::Internal(format!("Failed to fetch balance: {}", e))
        })?;

    // Get token mint address from environment
    let token_mint_str = std::env::var("GRID_TOKEN_MINT").map_err(|_| {
        error!("GRID_TOKEN_MINT environment variable not set");
        ApiError::Internal("Token mint address not configured".to_string())
    })?;

    let token_mint = Pubkey::from_str(&token_mint_str).map_err(|e| {
        error!("Invalid GRID_TOKEN_MINT address: {}", e);
        ApiError::Internal("Invalid token mint configuration".to_string())
    })?;

    // Derive the associated token account for this wallet
    let token_account = {
        use solana_sdk::pubkey::Pubkey as SdkPubkey;
        let wallet_bytes = wallet_pubkey.to_bytes();
        let mint_bytes = token_mint.to_bytes();

        // Use spl_token::solana_program for pubkey types
        let wallet_spl = spl_token::solana_program::pubkey::Pubkey::new_from_array(wallet_bytes);
        let mint_spl = spl_token::solana_program::pubkey::Pubkey::new_from_array(mint_bytes);

        let ata_spl =
            spl_associated_token_account::get_associated_token_address(&wallet_spl, &mint_spl);

        // Convert back to solana_sdk pubkey
        SdkPubkey::new_from_array(ata_spl.to_bytes())
    };

    info!(
        "Token account for wallet {}: {}",
        wallet_address, token_account
    );

    // Check if the token account exists and get its balance
    let (token_balance_raw, decimals) = match state
        .blockchain_service
        .account_exists(&token_account)
        .await
    {
        Ok(true) => {
            // Account exists, fetch the token balance
            let account_data = state
                .blockchain_service
                .get_account_data(&token_account)
                .await
                .map_err(|e| {
                    error!("Failed to fetch token account data: {}", e);
                    ApiError::Internal("Failed to fetch token account".to_string())
                })?;

            // Parse SPL Token Account data
            // SPL Token Account layout: amount (u64) at bytes 64-72
            if account_data.len() >= 72 {
                let amount = u64::from_le_bytes([
                    account_data[64],
                    account_data[65],
                    account_data[66],
                    account_data[67],
                    account_data[68],
                    account_data[69],
                    account_data[70],
                    account_data[71],
                ]);

                info!("Token balance for {}: {} raw units", wallet_address, amount);
                (amount, 9) // Energy tokens use 9 decimals (like SOL)
            } else {
                error!("Token account data too short: {} bytes", account_data.len());
                (0, 9)
            }
        }
        Ok(false) => {
            // Token account doesn't exist yet - balance is 0
            info!(
                "Token account does not exist for wallet: {}",
                wallet_address
            );
            (0, 9)
        }
        Err(e) => {
            error!("Failed to check token account existence: {}", e);
            return Err(ApiError::Internal(
                "Failed to check token account".to_string(),
            ));
        }
    };

    // Convert raw balance to human-readable format (divide by 10^decimals)
    let token_balance = token_balance_raw as f64 / 10_f64.powi(decimals as i32);

    info!(
        "Successfully fetched balance for wallet: {} - {} tokens ({} raw)",
        wallet_address, token_balance, token_balance_raw
    );

    Ok(Json(TokenBalanceResponse {
        wallet_address: wallet_address.clone(),
        token_balance: format!("{:.4}", token_balance),
        token_balance_raw,
        balance_sol: sol_balance,
        decimals,
        token_mint: token_mint.to_string(),
        token_account: token_account.to_string(),
    }))
}

/// Get token program info from blockchain
/// GET /api/tokens/info
#[utoipa::path(
    get,
    path = "/api/tokens/info",
    tag = "tokens",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Token program information", body = TokenInfoResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Token info not found on blockchain"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_token_info(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TokenInfoResponse>> {
    info!("Fetching token info from blockchain");

    // Get the Energy Token program ID
    let energy_token_program_id = crate::services::BlockchainService::energy_token_program_id()
        .map_err(|e| {
            error!("Failed to parse energy token program ID: {}", e);
            ApiError::Internal(format!("Invalid program ID: {}", e))
        })?;

    // Derive the token info PDA
    // Token info PDA seeds: ["token_info"]
    let (token_info_pda, _bump) =
        Pubkey::find_program_address(&[b"token_info"], &energy_token_program_id);

    info!("Token Info PDA: {}", token_info_pda);

    // Check if the account exists
    let account_exists = state
        .blockchain_service
        .account_exists(&token_info_pda)
        .await
        .map_err(|e| {
            error!("Failed to check if token info account exists: {}", e);
            ApiError::Internal(format!("Blockchain error: {}", e))
        })?;

    if !account_exists {
        return Err(ApiError::NotFound(
            "Token info account not found on blockchain".to_string(),
        ));
    }

    // Get the account data
    let account_data = state
        .blockchain_service
        .get_account_data(&token_info_pda)
        .await
        .map_err(|e| {
            error!("Failed to fetch token info account data: {}", e);
            ApiError::Internal(format!("Failed to fetch account: {}", e))
        })?;

    // Deserialize the account data (skip 8-byte discriminator)
    if account_data.len() < 8 {
        return Err(ApiError::Internal("Invalid account data".to_string()));
    }

    let token_info = parse_token_info(&account_data[8..]).map_err(|e| {
        error!("Failed to parse token info: {}", e);
        ApiError::Internal(format!("Failed to parse account data: {}", e))
    })?;

    info!("Successfully fetched token info");
    Ok(Json(token_info))
}

/// Parse token info from raw bytes
fn parse_token_info(data: &[u8]) -> Result<TokenInfoResponse> {
    // TokenInfo struct layout:
    // - authority: Pubkey (32 bytes)
    // - mint: Pubkey (32 bytes)
    // - total_supply: u64 (8 bytes)
    // - created_at: i64 (8 bytes)

    if data.len() < 80 {
        return Err(ApiError::Internal("Token info data too short".to_string()));
    }

    // Parse authority (first 32 bytes)
    let authority = Pubkey::try_from(&data[0..32])
        .map_err(|e| ApiError::Internal(format!("Invalid authority pubkey: {}", e)))?;

    // Parse mint (bytes 32-64)
    let mint = Pubkey::try_from(&data[32..64])
        .map_err(|e| ApiError::Internal(format!("Invalid mint pubkey: {}", e)))?;

    // Parse total_supply (bytes 64-72)
    let total_supply = u64::from_le_bytes([
        data[64], data[65], data[66], data[67], data[68], data[69], data[70], data[71],
    ]);

    // Parse created_at (bytes 72-80)
    let created_at = i64::from_le_bytes([
        data[72], data[73], data[74], data[75], data[76], data[77], data[78], data[79],
    ]);

    Ok(TokenInfoResponse {
        authority: authority.to_string(),
        mint: mint.to_string(),
        total_supply,
        created_at,
    })
}

/// Mint tokens request (admin only)
#[derive(Debug, Deserialize, ToSchema)]
pub struct MintTokensRequest {
    pub wallet_address: String,
    pub amount: u64,
}

/// Mint tokens response
#[derive(Debug, Serialize, ToSchema)]
pub struct MintTokensResponse {
    pub success: bool,
    pub message: String,
    pub wallet_address: String,
    pub amount: u64,
    pub transaction_signature: Option<String>,
}

/// Mint from meter reading request
#[derive(Debug, Deserialize, ToSchema)]
pub struct MintFromReadingRequest {
    pub reading_id: uuid::Uuid,
}

/// Mint from meter reading response
#[derive(Debug, Serialize, ToSchema)]
pub struct MintFromReadingResponse {
    pub success: bool,
    pub transaction_signature: String,
    pub reading_id: uuid::Uuid,
    pub tokens_minted: String,
    pub wallet_address: String,
}

/// Mint energy tokens (admin only)
/// POST /api/admin/tokens/mint
#[utoipa::path(
    post,
    path = "/api/admin/tokens/mint",
    tag = "tokens",
    request_body = MintTokensRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintTokensResponse),
        (status = 400, description = "Invalid request or amount"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn mint_tokens(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<MintTokensRequest>,
) -> Result<Json<MintTokensResponse>> {
    info!(
        "Mint tokens request from user {}: {} tokens to {}",
        user.0.sub, payload.amount, payload.wallet_address
    );

    // Validate amount
    if payload.amount == 0 {
        return Err(ApiError::BadRequest("Amount must be positive".to_string()));
    }

    // Validate wallet address
    let _wallet_pubkey = Pubkey::from_str(&payload.wallet_address).map_err(|e| {
        error!("Invalid wallet address: {}", e);
        ApiError::BadRequest(format!("Invalid wallet address: {}", e))
    })?;

    // Check user role - only admins can mint tokens
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can mint tokens".to_string(),
        ));
    }

    info!(
        "Mint tokens initiated: {} tokens to {}",
        payload.amount, payload.wallet_address
    );

    Ok(Json(MintTokensResponse {
        success: true,
        message: format!(
            "Token mint initiated for {} tokens to {}",
            payload.amount, payload.wallet_address
        ),
        wallet_address: payload.wallet_address,
        amount: payload.amount,
        transaction_signature: None,
    }))
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
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Tokens minted from reading successfully", body = MintFromReadingResponse),
        (status = 400, description = "Invalid request, reading already minted, or reading too old"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Can only mint own readings"),
        (status = 404, description = "Reading not found"),
        (status = 500, description = "Internal server error or blockchain minting failed")
    )
)]
pub async fn mint_from_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<MintFromReadingRequest>,
) -> Result<Json<MintFromReadingResponse>> {
    info!(
        "User {} minting tokens for reading {}",
        user.sub, request.reading_id
    );

    // Get the reading from database
    let reading = sqlx::query!(
        r#"
        SELECT 
            id, user_id, wallet_address, kwh_amount,
            minted, mint_tx_signature, reading_timestamp
        FROM meter_readings 
        WHERE id = $1
        "#,
        request.reading_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Database error fetching reading: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| {
        error!("Reading {} not found", request.reading_id);
        ApiError::NotFound("Meter reading not found".to_string())
    })?;

    // Verify ownership - user can only mint their own readings
    if reading.user_id != Some(user.sub) {
        return Err(ApiError::Forbidden(
            "You can only mint tokens from your own meter readings".to_string(),
        ));
    }

    // Check if already minted (double-claim prevention)
    if reading.minted.unwrap_or(false) {
        return Err(ApiError::BadRequest(format!(
            "This reading has already been minted. Transaction: {}",
            reading
                .mint_tx_signature
                .unwrap_or_else(|| "N/A".to_string())
        )));
    }

    // Validate the reading is recent (within 30 days)
    let thirty_days_ago = chrono::Utc::now() - chrono::Duration::days(30);
    if reading.reading_timestamp < Some(thirty_days_ago) {
        return Err(ApiError::BadRequest(
            "Reading is too old to mint (must be within 30 days)".to_string(),
        ));
    }

    // Parse wallet address
    let wallet_pubkey = Pubkey::from_str(&reading.wallet_address).map_err(|e| {
        error!("Invalid wallet address in reading: {}", e);
        ApiError::Internal("Invalid wallet address stored".to_string())
    })?;

    // Get authority keypair for minting
    let authority_keypair = state
        .wallet_service
        .get_authority_keypair()
        .await
        .map_err(|e| {
            error!("Failed to get authority keypair: {}", e);
            ApiError::Internal("Authority wallet not configured".to_string())
        })?;

    // Get token mint address from environment
    let token_mint_str = std::env::var("GRID_TOKEN_MINT").map_err(|_| {
        error!("GRID_TOKEN_MINT environment variable not set");
        ApiError::Internal("Token mint address not configured".to_string())
    })?;

    let token_mint = Pubkey::from_str(&token_mint_str).map_err(|e| {
        error!("Invalid GRID_TOKEN_MINT address: {}", e);
        ApiError::Internal("Invalid token mint configuration".to_string())
    })?;

    // Derive user's associated token account
    let user_token_account = {
        use solana_sdk::pubkey::Pubkey as SdkPubkey;
        let wallet_bytes = wallet_pubkey.to_bytes();
        let mint_bytes = token_mint.to_bytes();

        // Use spl_associated_token_account with correct types
        let wallet_spl = spl_token::solana_program::pubkey::Pubkey::new_from_array(wallet_bytes);
        let mint_spl = spl_token::solana_program::pubkey::Pubkey::new_from_array(mint_bytes);

        let ata_spl =
            spl_associated_token_account::get_associated_token_address(&wallet_spl, &mint_spl);

        // Convert back to solana_sdk pubkey
        SdkPubkey::new_from_array(ata_spl.to_bytes())
    };

    // Convert kWh amount to f64 for minting (1 kWh = 1 token)
    let kwh_amount = reading
        .kwh_amount
        .unwrap_or_default()
        .to_string()
        .parse::<f64>()
        .map_err(|e| {
            error!("Failed to parse kWh amount: {}", e);
            ApiError::Internal("Invalid kWh amount".to_string())
        })?;

    info!(
        "Minting {} kWh as tokens for wallet {} (token account: {})",
        kwh_amount, reading.wallet_address, user_token_account
    );

    // Call blockchain service to mint tokens
    let tx_signature = state
        .blockchain_service
        .mint_energy_tokens(
            &authority_keypair,
            &user_token_account,
            &token_mint,
            kwh_amount,
        )
        .await
        .map_err(|e| {
            error!("Blockchain minting failed: {}", e);
            ApiError::Internal(format!("Failed to mint tokens on blockchain: {}", e))
        })?;

    let tx_signature_str = tx_signature.to_string();
    info!(
        "Tokens minted successfully. Transaction: {}",
        tx_signature_str
    );

    // Update database - mark reading as minted
    sqlx::query!(
        r#"
        UPDATE meter_readings 
        SET minted = TRUE,
            mint_tx_signature = $1,
            updated_at = NOW()
        WHERE id = $2
        "#,
        tx_signature_str,
        request.reading_id
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to update reading as minted: {}", e);
        ApiError::Database(e)
    })?;

    info!(
        "Reading {} marked as minted. User {} received {} tokens",
        request.reading_id, user.sub, kwh_amount
    );

    Ok(Json(MintFromReadingResponse {
        success: true,
        transaction_signature: tx_signature_str,
        reading_id: request.reading_id,
        tokens_minted: kwh_amount.to_string(),
        wallet_address: reading.wallet_address,
    }))
}
