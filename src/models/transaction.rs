// Enhanced transaction models for unified blockchain transaction tracking

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Types of blockchain transactions
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema, PartialEq, Eq)]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum TransactionType {
    EnergyTrade,
    TokenMint,
    TokenTransfer,
    GovernanceVote,
    OracleUpdate,
    RegistryUpdate,
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnergyTrade => write!(f, "energy_trade"),
            Self::TokenMint => write!(f, "token_mint"),
            Self::TokenTransfer => write!(f, "token_transfer"),
            Self::GovernanceVote => write!(f, "governance_vote"),
            Self::OracleUpdate => write!(f, "oracle_update"),
            Self::RegistryUpdate => write!(f, "registry_update"),
        }
    }
}

impl std::str::FromStr for TransactionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "energy_trade" => Ok(Self::EnergyTrade),
            "token_mint" => Ok(Self::TokenMint),
            "token_transfer" => Ok(Self::TokenTransfer),
            "governance_vote" => Ok(Self::GovernanceVote),
            "oracle_update" => Ok(Self::OracleUpdate),
            "registry_update" => Ok(Self::RegistryUpdate),
            _ => Err(format!("Unknown transaction type: {}", s)),
        }
    }
}

impl TransactionType {
    /// Convert to string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::EnergyTrade => "energy_trade",
            Self::TokenMint => "token_mint",
            Self::TokenTransfer => "token_transfer",
            Self::GovernanceVote => "governance_vote",
            Self::OracleUpdate => "oracle_update",
            Self::RegistryUpdate => "registry_update",
        }
    }
}

/// Transaction status
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema, PartialEq, Eq)]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum TransactionStatus {
    Pending,
    Processing,
    Submitted,
    Confirmed,
    Failed,
    Settled,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Submitted => write!(f, "submitted"),
            Self::Confirmed => write!(f, "confirmed"),
            Self::Failed => write!(f, "failed"),
            Self::Settled => write!(f, "settled"),
        }
    }
}

impl std::str::FromStr for TransactionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "submitted" => Ok(Self::Submitted),
            "confirmed" => Ok(Self::Confirmed),
            "failed" => Ok(Self::Failed),
            "settled" => Ok(Self::Settled),
            _ => Err(format!("Unknown transaction status: {}", s)),
        }
    }
}

/// Order type for energy trades
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum OrderType {
    Sell,
    Buy,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sell => write!(f, "sell"),
            Self::Buy => write!(f, "buy"),
        }
    }
}

impl std::str::FromStr for OrderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sell" => Ok(Self::Sell),
            "buy" => Ok(Self::Buy),
            _ => Err(format!("Unknown order type: {}", s)),
        }
    }
}

/// Create transaction request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTransactionRequest {
    pub transaction_type: TransactionType,
    pub user_id: Uuid,
    pub payload: TransactionPayload,
    pub max_priority_fee: Option<u64>,
    pub skip_prevalidation: bool,
}

/// Transaction payload variants
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum TransactionPayload {
    EnergyTrade {
        market_pubkey: String,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: OrderType,
        erc_certificate_id: Option<String>,
    },
    TokenMint {
        recipient: String,
        amount: u64,
    },
    TokenTransfer {
        from: String,
        to: String,
        amount: u64,
        token_mint: String,
    },
    GovernanceVote {
        proposal_id: u64,
        vote: bool,
    },
    OracleUpdate {
        price_feed_id: String,
        price: u64,
        confidence: u64,
    },
    RegistryUpdate {
        participant_id: String,
        update_data: serde_json::Value,
    },
}

/// Transaction response
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TransactionResponse {
    pub operation_id: Uuid,
    pub transaction_type: TransactionType,
    pub user_id: Option<Uuid>,
    pub status: TransactionStatus,
    pub signature: Option<String>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
}

/// Transaction statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionStats {
    pub total_count: i64,
    pub pending_count: i64,
    pub processing_count: i64,
    pub submitted_count: i64,
    pub confirmed_count: i64,
    pub failed_count: i64,
    pub settled_count: i64,
    pub avg_confirmation_time_seconds: Option<f64>,
    pub success_rate: f64,
}

/// Filters for querying transactions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionFilters {
    pub operation_type: Option<TransactionType>,
    pub tx_type: Option<TransactionType>,
    pub status: Option<TransactionStatus>,
    pub user_id: Option<Uuid>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_attempts: Option<i32>,
    pub has_signature: Option<bool>,
}

impl Default for TransactionFilters {
    fn default() -> Self {
        Self {
            operation_type: None,
            tx_type: None,
            status: None,
            user_id: None,
            date_from: None,
            date_to: None,
            limit: Some(100),
            offset: Some(0),
            min_attempts: None,
            has_signature: None,
        }
    }
}

/// Transaction retry request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionRetryRequest {
    pub operation_id: Uuid,
    pub operation_type: Option<String>,
    pub max_attempts: Option<i32>,
}

/// Transaction retry response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionRetryResponse {
    pub success: bool,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub signature: Option<String>,
    pub status: TransactionStatus,
}

/// Transaction monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionMonitoringConfig {
    pub enabled: bool,
    pub monitoring_interval: u64,
    pub max_retry_attempts: i32,
    pub max_status_check_attempts: i32,
    pub retry_delay_seconds: i32,
    pub transaction_expiry_seconds: u64,
    pub max_priority_fee: u64,
}

impl Default for TransactionMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitoring_interval: 10,         // 10 seconds
            max_retry_attempts: 3,           // 3 attempts
            max_status_check_attempts: 5,    // 5 attempts
            retry_delay_seconds: 30,         // 30 seconds
            transaction_expiry_seconds: 300, // 5 minutes
            max_priority_fee: 100000,        // 0.0001 SOL
        }
    }
}

/// Blockchain operation record from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BlockchainOperation {
    pub operation_id: Uuid,
    pub operation_type: TransactionType,
    pub tx_type: Option<String>,
    pub user_id: Option<Uuid>,
    pub status: TransactionStatus,
    pub operation_status: Option<String>,
    pub signature: Option<String>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub payload: serde_json::Value,
    pub max_priority_fee: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

impl BlockchainOperation {
    /// Create a new blockchain operation
    pub fn new(
        operation_id: Uuid,
        operation_type: TransactionType,
        user_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            operation_id,
            operation_type,
            tx_type: None,
            user_id,
            status: TransactionStatus::Pending,
            operation_status: None,
            signature: None,
            attempts: 0,
            last_error: None,
            payload,
            max_priority_fee: None,
            created_at: now,
            updated_at: now,
            submitted_at: None,
            confirmed_at: None,
        }
    }

    /// Check if transaction is in a retryable state
    pub fn can_retry(&self) -> bool {
        matches!(
            self.status,
            TransactionStatus::Pending | TransactionStatus::Failed
        ) && self.attempts < 3 // Default max attempts
    }

    /// Check if transaction is expired
    pub fn is_expired(&self, expiry_seconds: u64) -> bool {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.created_at).num_seconds();
        elapsed > expiry_seconds as i64
    }
}

/// Energy trade payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EnergyTradePayload {
    pub market_pubkey: String,
    pub energy_amount: u64,
    pub price_per_kwh: u64,
    pub order_type: OrderType,
    pub erc_certificate_id: Option<String>,
}

/// Token mint payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMintPayload {
    pub recipient: String,
    pub amount: u64,
}

/// Token transfer payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenTransferPayload {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub token_mint: String,
}

/// Governance vote payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GovernanceVotePayload {
    pub proposal_id: u64,
    pub vote: bool,
}

/// Oracle update payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OracleUpdatePayload {
    pub price_feed_id: String,
    pub price: u64,
    pub confidence: u64,
}

/// Registry update payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegistryUpdatePayload {
    pub participant_id: String,
    pub update_data: serde_json::Value,
}

/// Trade record for energy trading transactions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TradeRecord {
    pub sell_order: String,
    pub buy_order: String,
    pub seller: String,
    pub buyer: String,
    pub amount: u64,
    pub price_per_kwh: u64,
    pub total_value: u64,
    pub fee_amount: u64,
    pub executed_at: i64,
}

/// Transaction validation error
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

impl ValidationError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: None,
        }
    }

    pub fn with_field(code: &str, message: &str, field: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: Some(field.to_string()),
        }
    }
}
