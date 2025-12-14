use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Settlement status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SettlementStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl std::fmt::Display for SettlementStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Settlement record
#[derive(Debug, Clone, Serialize)]
pub struct Settlement {
    pub id: Uuid,
    pub trade_id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    // Add missing fields for PDA lookup
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub energy_amount: Decimal,
    pub price: Decimal,
    pub total_value: Decimal,
    pub fee_amount: Decimal,
    pub net_amount: Decimal,
    pub status: SettlementStatus,
    pub blockchain_tx: Option<String>,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// Settlement transaction result
#[derive(Debug, Clone, Serialize)]
pub struct SettlementTransaction {
    pub settlement_id: Uuid,
    pub signature: String,
    pub slot: u64,
    pub confirmation_status: String,
}

/// Settlement service configuration
#[derive(Debug, Clone)]
pub struct SettlementConfig {
    pub fee_rate: Decimal,            // Platform fee (e.g., 0.01 = 1%)
    pub min_confirmation_blocks: u64, // Minimum blocks for confirmation
    pub retry_attempts: u32,          // Number of retry attempts for failed transactions
    pub retry_delay_secs: u64,        // Delay between retries
    pub enable_real_blockchain: bool, // Enable/disable real blockchain interactions
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            fee_rate: Decimal::from_str("0.01").unwrap(), // 1% platform fee
            min_confirmation_blocks: 32,                  // ~13 seconds on Solana
            retry_attempts: 3,
            retry_delay_secs: 5,
            enable_real_blockchain: true, // Default to true for safety
        }
    }
}

/// Settlement statistics
#[derive(Debug, Clone, Serialize)]
pub struct SettlementStats {
    pub pending_count: i64,
    pub processing_count: i64,
    pub confirmed_count: i64,
    pub failed_count: i64,
    pub total_settled_value: Decimal,
}
