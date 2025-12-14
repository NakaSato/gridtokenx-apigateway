use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionSubmission {
    pub transaction: String, // base64 encoded transaction
    pub program_id: String,
    #[schema(value_type = f64)]
    pub priority_fee: rust_decimal::Decimal,
    pub compute_units: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionStatus {
    pub signature: String,
    pub status: String,
    pub block_height: Option<u64>,
    pub confirmation_status: String,
    #[schema(value_type = f64)]
    pub fee: rust_decimal::Decimal,
    pub compute_units_consumed: Option<u32>,
    pub logs: Vec<String>,
    pub program_interactions: Vec<ProgramInteraction>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProgramInteraction {
    pub program_id: String,
    pub instruction_name: String,
    pub success: bool,
}