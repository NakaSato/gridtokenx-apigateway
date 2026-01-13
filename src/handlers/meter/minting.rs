//! Token minting from meter readings

use axum::{extract::{State, Path}, Json};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    services::BlockchainService,
    AppState,
};

use super::types::{MintFromReadingRequest, MintResponse};

/// Inline role check (since require_role is in disabled module)
fn check_admin_role(user: &crate::auth::Claims) -> Result<()> {
    if user.role.to_lowercase() != "admin" {
        return Err(ApiError::Forbidden(
            "Access denied. Admin role required.".to_string(),
        ));
    }
    Ok(())
}

/// Helper to get reading by ID directly from database
async fn get_reading_by_id(db: &sqlx::PgPool, reading_id: Uuid) -> Result<MeterReadingRecord> {
    sqlx::query_as!(
        MeterReadingRecord,
        r#"
        SELECT id, user_id, wallet_address, kwh_amount, minted, mint_tx_signature
        FROM meter_readings
        WHERE id = $1
        "#,
        reading_id
    )
    .fetch_optional(db)
    .await
    .map_err(|e| {
        error!("Database error fetching reading: {}", e);
        ApiError::Internal("Database error".to_string())
    })?
    .ok_or_else(|| ApiError::NotFound("Reading not found".to_string()))
}

/// Helper to mark reading as minted
async fn mark_as_minted(db: &sqlx::PgPool, reading_id: Uuid, tx_signature: &str) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE meter_readings 
        SET minted = true, mint_tx_signature = $2
        WHERE id = $1
        "#,
        reading_id,
        tx_signature
    )
    .execute(db)
    .await
    .map_err(|e| {
        error!("Failed to update reading: {}", e);
        ApiError::Internal("Failed to update reading status".to_string())
    })?;
    Ok(())
}

/// Internal reading record for database queries
#[derive(Debug)]
#[allow(dead_code)]
struct MeterReadingRecord {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    pub kwh_amount: Option<Decimal>,
    pub minted: Option<bool>,
    pub mint_tx_signature: Option<String>,
}

/// Mint tokens from a meter reading (admin only)
/// POST /api/admin/meters/mint-from-reading
///
/// This endpoint mints energy tokens based on a submitted meter reading
#[utoipa::path(
    post,
    path = "/api/admin/meters/mint-from-reading",
    tag = "meters",
    request_body = MintFromReadingRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintResponse),
        (status = 400, description = "Invalid reading or already minted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "Reading not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn mint_from_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<MintFromReadingRequest>,
) -> Result<Json<MintResponse>> {
    // Check admin permission
    check_admin_role(&user)?;

    info!(
        "Admin {} minting tokens for reading {}",
        user.sub, request.reading_id
    );

    // Get reading details
    let reading = get_reading_by_id(&state.db, request.reading_id).await?;

    // Check if already minted
    if reading.minted.unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "Reading has already been minted".to_string(),
        ));
    }

    let kwh_amount = reading
        .kwh_amount
        .ok_or_else(|| ApiError::Internal("Missing kwh_amount".to_string()))?;

    let wallet_address = reading.wallet_address.clone();

    // Get authority keypair
    let authority_keypair = state
        .wallet_service
        .get_authority_keypair()
        .await
        .map_err(|e| {
            error!("Failed to get authority keypair: {}", e);
            ApiError::Internal("Failed to access blockchain".to_string())
        })?;

    // Parse addresses
    let token_mint = BlockchainService::parse_pubkey(&state.config.energy_token_mint)
        .map_err(|e| ApiError::Internal(format!("Invalid token mint: {}", e)))?;

    let wallet_pubkey = BlockchainService::parse_pubkey(&wallet_address)
        .map_err(|e| ApiError::BadRequest(format!("Invalid wallet address: {}", e)))?;

    // Ensure user token account exists
    let _user_token_account = state
        .blockchain_service
        .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
        .await
        .map_err(|e| {
            error!("Failed to ensure token account: {}", e);
            ApiError::Internal("Failed to create token account".to_string())
        })?;

    // Mint tokens
    let amount_f64 = kwh_amount
        .to_f64()
        .ok_or_else(|| ApiError::Internal("Failed to convert amount".to_string()))?;

    // Mint tokens using appropriate method based on config
    let signature = if state.config.tokenization.enable_real_blockchain {
        state
            .blockchain_service
            .mint_energy_tokens(
                &authority_keypair,
                &_user_token_account,
                &wallet_pubkey,
                &token_mint,
                amount_f64,
            )
            .await
            .map_err(|e| {
                error!("Failed to mint tokens (Anchor): {}", e);
                ApiError::Internal(format!("Blockchain minting failed: {}", e))
            })?
    } else {
        state
            .blockchain_service
            .mint_spl_tokens(
                &authority_keypair,
                &wallet_pubkey,
                &token_mint,
                amount_f64,
            )
            .await
            .map_err(|e| {
                error!("Failed to mint tokens (CLI): {}", e);
                ApiError::Internal(format!("Blockchain minting failed: {}", e))
            })?
    };

    let sig_str = signature.to_string();
    info!(
        "Minted {} kWh for reading {}: {}",
        amount_f64, request.reading_id, sig_str
    );

    // Mark reading as minted
    mark_as_minted(&state.db, request.reading_id, &sig_str).await?;

    Ok(Json(MintResponse {
        message: "Tokens minted successfully".to_string(),
        transaction_signature: sig_str,
        kwh_amount,
        wallet_address,
    }))
}

/// Mint tokens from a user's own meter reading
/// POST /api/v1/meters/readings/{reading_id}/mint
///
/// This endpoint allows authenticated users to mint tokens from their own readings
#[utoipa::path(
    post,
    path = "/api/v1/meters/readings/{reading_id}/mint",
    tag = "meters",
    params(
        ("reading_id" = String, Path, description = "Reading ID (UUID) to mint")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintResponse),
        (status = 400, description = "Invalid reading or already minted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - You can only mint your own readings"),
        (status = 404, description = "Reading not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn mint_user_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(reading_id): Path<Uuid>,
) -> Result<Json<MintResponse>> {
    info!(
        "User {} requesting to mint tokens for reading {}",
        user.sub, reading_id
    );

    // Get reading details
    let reading = get_reading_by_id(&state.db, reading_id).await?;

    // Verify ownership - user can only mint their own readings
    let reading_user_id = reading.user_id.ok_or_else(|| {
        ApiError::BadRequest("Reading has no associated user".to_string())
    })?;
    if reading_user_id != user.sub {
        return Err(ApiError::Forbidden(
            "You can only mint your own readings".to_string(),
        ));
    }

    // Check if already minted
    if reading.minted.unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "Reading has already been minted".to_string(),
        ));
    }

    let kwh_amount = reading
        .kwh_amount
        .ok_or_else(|| ApiError::Internal("Missing kwh_amount".to_string()))?;

    let wallet_address = reading.wallet_address.clone();

    // Get authority keypair
    let authority_keypair = state
        .wallet_service
        .get_authority_keypair()
        .await
        .map_err(|e| {
            error!("Failed to get authority keypair: {}", e);
            ApiError::Internal("Failed to access blockchain".to_string())
        })?;

    // Parse addresses
    info!("Using token mint: {}", state.config.energy_token_mint);
    let token_mint = BlockchainService::parse_pubkey(&state.config.energy_token_mint)
        .map_err(|e| ApiError::Internal(format!("Invalid token mint: {}", e)))?;

    let wallet_pubkey = BlockchainService::parse_pubkey(&wallet_address)
        .map_err(|e| ApiError::BadRequest(format!("Invalid wallet address: {}", e)))?;

    // Ensure user token account exists
    let _user_token_account = state
        .blockchain_service
        .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
        .await
        .map_err(|e| {
            error!("Failed to ensure token account: {}", e);
            ApiError::Internal("Failed to create token account".to_string())
        })?;

    // Mint tokens
    let amount_f64 = kwh_amount
        .to_f64()
        .ok_or_else(|| ApiError::Internal("Failed to convert amount".to_string()))?;

    // Mint tokens using appropriate method based on config
    let signature = if state.config.tokenization.enable_real_blockchain {
        state
            .blockchain_service
            .mint_energy_tokens(
                &authority_keypair,
                &_user_token_account,
                &wallet_pubkey,
                &token_mint,
                amount_f64,
            )
            .await
            .map_err(|e| {
                error!("Failed to mint tokens (Anchor): {}", e);
                ApiError::Internal(format!("Blockchain minting failed: {}", e))
            })?
    } else {
        state
            .blockchain_service
            .mint_spl_tokens(
                &authority_keypair,
                &wallet_pubkey,
                &token_mint,
                amount_f64,
            )
            .await
            .map_err(|e| {
                error!("Failed to mint tokens (CLI): {}", e);
                ApiError::Internal(format!("Blockchain minting failed: {}", e))
            })?
    };

    let sig_str = signature.to_string();
    info!(
        "User {} minted {} kWh for reading {}: {}",
        user.sub, amount_f64, reading_id, sig_str
    );

    // Mark reading as minted
    mark_as_minted(&state.db, reading_id, &sig_str).await?;

    Ok(Json(MintResponse {
        message: "Tokens minted successfully".to_string(),
        transaction_signature: sig_str,
        kwh_amount,
        wallet_address,
    }))
}

