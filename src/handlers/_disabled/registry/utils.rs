use solana_sdk::pubkey::Pubkey;

use super::types::{BlockchainUserAccount, UserStatus, UserType};
use crate::error::{ApiError, Result};

/// Parse user account data from raw bytes
/// This is a simplified parser - in production, use Anchor's generated client
pub fn parse_user_account(data: &[u8]) -> Result<BlockchainUserAccount> {
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
