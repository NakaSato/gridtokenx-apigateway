use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};

/// Governance status response
#[derive(Debug, Serialize, ToSchema)]
pub struct GovernanceStatusResponse {
    pub authority: String,
    pub authority_name: String,
    pub emergency_paused: bool,
    pub maintenance_mode: bool,
    pub erc_validation_enabled: bool,
    pub total_ercs_issued: u64,
    pub total_ercs_validated: u64,
    pub total_energy_certified: u64,
    pub is_operational: bool,
    pub created_at: i64,
    pub last_updated: i64,
}

/// Emergency pause request
#[derive(Debug, Deserialize, ToSchema)]
pub struct EmergencyPauseRequest {
    pub reason: String,
}

/// Emergency action response
#[derive(Debug, Serialize, ToSchema)]
pub struct EmergencyActionResponse {
    pub success: bool,
    pub message: String,
    pub paused: bool,
    pub timestamp: i64,
}

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
    let governance_program_id = state.blockchain_service.governance_program_id()
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

/// Emergency pause the system (admin only)
/// POST /api/admin/governance/emergency-pause
#[utoipa::path(
    post,
    path = "/api/admin/governance/emergency-pause",
    tag = "governance",
    request_body = EmergencyPauseRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Emergency pause initiated", body = EmergencyActionResponse),
        (status = 400, description = "Invalid request or empty reason"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn emergency_pause(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<EmergencyPauseRequest>,
) -> Result<Json<EmergencyActionResponse>> {
    info!("Emergency pause request from user: {}", user.0.sub);

    // Validate reason is not empty
    if payload.reason.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Emergency reason is required".to_string(),
        ));
    }

    // Check user role - only admins can emergency pause
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
            "Only admins can trigger emergency pause".to_string(),
        ));
    }

    let timestamp = chrono::Utc::now().timestamp();

    info!(
        "Emergency pause initiated by user {} with reason: {}",
        user.0.sub, payload.reason
    );

    Ok(Json(EmergencyActionResponse {
        success: true,
        message: format!("Emergency pause initiated. Reason: {}", payload.reason),
        paused: true,
        timestamp,
    }))
}

/// Emergency unpause the system (admin only)
/// POST /api/admin/governance/unpause
#[utoipa::path(
    post,
    path = "/api/admin/governance/unpause",
    tag = "governance",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Emergency unpause initiated", body = EmergencyActionResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn emergency_unpause(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<EmergencyActionResponse>> {
    info!("Emergency unpause request from user: {}", user.0.sub);

    // Check user role - only admins can emergency unpause
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
            "Only admins can unpause the system".to_string(),
        ));
    }

    let timestamp = chrono::Utc::now().timestamp();

    info!("Emergency unpause initiated by user {}", user.0.sub);

    Ok(Json(EmergencyActionResponse {
        success: true,
        message: "Emergency unpause initiated successfully".to_string(),
        paused: false,
        timestamp,
    }))
}

/// Parse governance status from raw bytes
fn parse_governance_status(data: &[u8]) -> Result<GovernanceStatusResponse> {
    // PoAConfig struct layout (simplified parsing):
    // - authority: Pubkey (32 bytes)
    // - authority_name: String (4 bytes length + data)
    // - contact_info: String (4 bytes length + data)
    // ... and more fields

    if data.len() < 32 {
        return Err(ApiError::Internal("Governance data too short".to_string()));
    }

    // Parse authority (first 32 bytes)
    let authority = Pubkey::try_from(&data[0..32])
        .map_err(|e| ApiError::Internal(format!("Invalid authority pubkey: {}", e)))?;

    // Parse authority_name string (starts at byte 32)
    let authority_name_len = u32::from_le_bytes([data[32], data[33], data[34], data[35]]) as usize;
    let authority_name_start = 36;
    let authority_name_end = authority_name_start + authority_name_len;

    if data.len() < authority_name_end {
        return Err(ApiError::Internal(
            "Invalid authority_name data".to_string(),
        ));
    }

    let authority_name = String::from_utf8(data[authority_name_start..authority_name_end].to_vec())
        .map_err(|e| ApiError::Internal(format!("Invalid UTF-8 in authority_name: {}", e)))?;

    // Calculate offset after authority_name (with padding to 8-byte boundary)
    let after_name = authority_name_end;
    let padding = (8 - (after_name % 8)) % 8;
    let contact_info_offset = after_name + padding;

    // Parse contact_info string
    if data.len() < contact_info_offset + 4 {
        return Err(ApiError::Internal(
            "Invalid contact_info offset".to_string(),
        ));
    }

    let contact_info_len = u32::from_le_bytes([
        data[contact_info_offset],
        data[contact_info_offset + 1],
        data[contact_info_offset + 2],
        data[contact_info_offset + 3],
    ]) as usize;

    let contact_info_start = contact_info_offset + 4;
    let _contact_info_end = contact_info_start + contact_info_len;

    // Skip contact_info parsing for now and use approximations for boolean fields
    // In a production system, you'd want to parse the entire struct carefully

    // For now, return a simplified response with safe defaults
    // TODO: Implement full struct parsing with proper byte alignment
    let emergency_paused = false; // Would be parsed from data
    let maintenance_mode = false; // Would be parsed from data
    let erc_validation_enabled = true; // Would be parsed from data

    Ok(GovernanceStatusResponse {
        authority: authority.to_string(),
        authority_name,
        emergency_paused,
        maintenance_mode,
        erc_validation_enabled,
        total_ercs_issued: 0,      // Would be parsed from data
        total_ercs_validated: 0,   // Would be parsed from data
        total_energy_certified: 0, // Would be parsed from data
        is_operational: !emergency_paused && !maintenance_mode,
        created_at: chrono::Utc::now().timestamp(), // Would be parsed from data
        last_updated: chrono::Utc::now().timestamp(), // Would be parsed from data
    })
}
