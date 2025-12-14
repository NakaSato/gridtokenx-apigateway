use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SwapTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub pool_id: Uuid,
    pub input_token: String,
    pub input_amount: Decimal,
    pub output_token: String,
    pub output_amount: Decimal,
    pub fee_amount: Decimal,
    pub slippage_tolerance: Option<Decimal>,
    pub status: String,
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}
