use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
