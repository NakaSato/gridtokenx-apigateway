use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::{debug, error, info};
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};

/// User type enum matching the Registry program
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserType {
    Prosumer,
    Consumer,
}

/// User status enum matching the Registry program
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    Active,
    Suspended,
    Inactive,
}

/// Meter type enum matching the Registry program
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MeterType {
    Solar,
    Wind,
    Battery,
    Grid,
}

/// Meter status enum matching the Registry program
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MeterStatus {
    Active,
    Inactive,
    Maintenance,
}

/// User account data from blockchain
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BlockchainUserAccount {
    pub authority: String,
    pub user_type: UserType,
    pub location: String,
    pub status: UserStatus,
    pub registered_at: i64,
    pub meter_count: u32,
}

/// Meter account data from blockchain
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BlockchainMeterAccount {
    pub meter_id: String,
    pub owner: String,
    pub meter_type: MeterType,
    pub status: MeterStatus,
    pub registered_at: i64,
    pub last_reading_at: i64,
    pub total_generation: u64,
    pub total_consumption: u64,
    pub settled_net_generation: u64,
    pub claimed_erc_generation: u64,
}

/// Request to update user role
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUserRoleRequest {
    pub new_status: UserStatus,
}

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
    let registry_program_id =
        state.blockchain_service.registry_program_id().map_err(|e| {
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

/// Update user role/status (admin only)
/// POST /api/admin/users/:id/update-role
#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/update-role",
    tag = "registry",
    request_body = UpdateUserRoleRequest,
    security(("bearer_auth" = [])),
    params(
        ("id" = uuid::Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User role update initiated"),
        (status = 400, description = "Invalid request or user has no wallet"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_user_role(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(user_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateUserRoleRequest>,
) -> Result<Json<serde_json::Value>> {
    info!(
        "Admin {} updating user {} role to {:?}",
        user.0.sub, user_id, payload.new_status
    );

    // Check if user is admin (in database)
    let db_user = sqlx::query!(
        "SELECT id, email, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch admin user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("Admin user not found".to_string()))?;

    // Verify admin role
    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can update user roles".to_string(),
        ));
    }

    // Get the target user from database
    let target_user = sqlx::query!(
        "SELECT id, email, wallet_address, role::text as role FROM users WHERE id = $1",
        user_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch target user: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("Target user not found".to_string()))?;

    // Get wallet address
    let wallet_address = target_user
        .wallet_address
        .ok_or_else(|| ApiError::BadRequest("User has no wallet address".to_string()))?;

    info!(
        "Would update blockchain status for wallet {} to {:?}",
        wallet_address, payload.new_status
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "User role update initiated",
        "user_id": user_id,
        "wallet_address": wallet_address,
        "new_status": payload.new_status,
        "note": "On-chain transaction not yet implemented - requires transaction signing"
    })))
}

/// Parse user account data from raw bytes
/// This is a simplified parser - in production, use Anchor's generated client
fn parse_user_account(data: &[u8]) -> Result<BlockchainUserAccount> {
    // UserAccount struct layout (after 8-byte discriminator):
    // - authority: Pubkey (32 bytes)
    // - user_type: u8 (1 byte + padding)
    // - location: String (4 bytes length + data)
    // - status: u8 (1 byte + padding)
    // - registered_at: i64 (8 bytes)
    // - meter_count: u32 (4 bytes)
    // - created_at: i64 (8 bytes)

    if data.len() < 32 {
        return Err(ApiError::Internal("Account data too short".to_string()));
    }

    // Parse authority (first 32 bytes)
    let authority = Pubkey::try_from(&data[0..32])
        .map_err(|e| ApiError::Internal(format!("Invalid pubkey: {}", e)))?;

    // Parse user_type (byte 32)
    let user_type = match data.get(32) {
        Some(0) => UserType::Prosumer,
        Some(1) => UserType::Consumer,
        _ => return Err(ApiError::Internal("Invalid user type".to_string())),
    };

    // Parse location string (starts at byte 36 for alignment)
    // String format: 4 bytes length + data
    let location_len = u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let location_start = 40;
    let location_end = location_start + location_len;

    if data.len() < location_end {
        return Err(ApiError::Internal("Invalid location data".to_string()));
    }

    let location = String::from_utf8(data[location_start..location_end].to_vec())
        .map_err(|e| ApiError::Internal(format!("Invalid UTF-8: {}", e)))?;

    // Calculate offset after string (with padding to 8-byte boundary)
    let after_string = location_end;
    let padding = (8 - (after_string % 8)) % 8;
    let status_offset = after_string + padding;

    // Parse status
    let status = match data.get(status_offset) {
        Some(0) => UserStatus::Active,
        Some(1) => UserStatus::Suspended,
        Some(2) => UserStatus::Inactive,
        _ => return Err(ApiError::Internal("Invalid user status".to_string())),
    };

    // Parse registered_at (8 bytes after status + alignment)
    let registered_at_offset = status_offset + 8; // 1 byte + 7 padding
    if data.len() < registered_at_offset + 8 {
        return Err(ApiError::Internal("Invalid registered_at data".to_string()));
    }
    let registered_at = i64::from_le_bytes([
        data[registered_at_offset],
        data[registered_at_offset + 1],
        data[registered_at_offset + 2],
        data[registered_at_offset + 3],
        data[registered_at_offset + 4],
        data[registered_at_offset + 5],
        data[registered_at_offset + 6],
        data[registered_at_offset + 7],
    ]);

    // Parse meter_count (4 bytes)
    let meter_count_offset = registered_at_offset + 8;
    if data.len() < meter_count_offset + 4 {
        return Err(ApiError::Internal("Invalid meter_count data".to_string()));
    }
    let meter_count = u32::from_le_bytes([
        data[meter_count_offset],
        data[meter_count_offset + 1],
        data[meter_count_offset + 2],
        data[meter_count_offset + 3],
    ]);

    Ok(BlockchainUserAccount {
        authority: authority.to_string(),
        user_type,
        location,
        status,
        registered_at,
        meter_count,
    })
}
