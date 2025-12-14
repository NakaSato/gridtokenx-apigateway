use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
