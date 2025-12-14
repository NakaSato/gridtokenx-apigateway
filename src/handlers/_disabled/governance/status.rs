use axum::{extract::State, response::Json};
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};

use super::types::GovernanceStatusResponse;
use super::utils::parse_governance_status;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Get governance status from blockchain
/// GET /api/governance/status
#[utoipa::path(
    get,
    path = "/api/governance/status",
    tag = "governance",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Governance system status", body = GovernanceStatusResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Governance config not found on blockchain"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_governance_status(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<GovernanceStatusResponse>> {
    info!("Fetching governance status from blockchain");

    // Get the Governance program ID
    let governance_program_id = state
        .blockchain_service
        .governance_program_id()
        .map_err(|e| {
            error!("Failed to parse governance program ID: {}", e);
            ApiError::Internal(format!("Invalid program ID: {}", e))
        })?;

    // Derive the PoA config PDA
    // PoA config PDA seeds: ["poa_config"]
    let (poa_config_pda, _bump) =
        Pubkey::find_program_address(&[b"poa_config"], &governance_program_id);

    info!("PoA Config PDA: {}", poa_config_pda);

    // Check if the account exists
    let account_exists = state
        .blockchain_service
        .account_exists(&poa_config_pda)
        .await
        .map_err(|e| {
            error!("Failed to check if governance account exists: {}", e);
            ApiError::Internal(format!("Blockchain error: {}", e))
        })?;

    if !account_exists {
        return Err(ApiError::NotFound(
            "Governance config not found on blockchain".to_string(),
        ));
    }

    // Get the account data
    let account_data = state
        .blockchain_service
        .get_account_data(&poa_config_pda)
        .await
        .map_err(|e| {
            error!("Failed to fetch governance account data: {}", e);
            ApiError::Internal(format!("Failed to fetch account: {}", e))
        })?;

    // Deserialize the account data (skip 8-byte discriminator)
    if account_data.len() < 8 {
        return Err(ApiError::Internal("Invalid account data".to_string()));
    }

    let governance_status = parse_governance_status(&account_data[8..]).map_err(|e| {
        error!("Failed to parse governance status: {}", e);
        ApiError::Internal(format!("Failed to parse account data: {}", e))
    })?;

    info!("Successfully fetched governance status");
    Ok(Json(governance_status))
}
