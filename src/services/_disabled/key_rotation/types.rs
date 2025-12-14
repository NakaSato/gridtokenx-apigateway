use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Report of a key rotation operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotationReport {
    pub total_users: usize,
    pub successful: usize,
    pub failed: usize,
    pub duration_seconds: f64,
    pub errors: Vec<String>,
    pub new_version: i32,
}

/// Current status of key rotation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotationStatus {
    pub current_version: i32,
    pub total_keys: i32,
    pub active_key_version: i32,
    pub users_by_version: Vec<(i32, i64)>,
}
