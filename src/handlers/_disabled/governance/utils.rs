use solana_sdk::pubkey::Pubkey;

use super::types::GovernanceStatusResponse;
use crate::error::{ApiError, Result};

/// Parse governance status from raw bytes
pub fn parse_governance_status(data: &[u8]) -> Result<GovernanceStatusResponse> {
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
