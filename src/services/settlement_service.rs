// Settlement service for executing blockchain transactions for matched trades
// Handles energy token transfers on Solana blockchain

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::market_clearing::TradeMatch;
use crate::services::BlockchainService;

/// Settlement status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SettlementStatus {
    Pending,
    Processing,
    Confirmed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for SettlementStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Processing => write!(f, "Processing"),
            Self::Confirmed => write!(f, "Confirmed"),
            Self::Failed => write!(f, "Failed"),
            Self::Cancelled => write!(f, "Cancelled"),
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
    pub fee_rate: Decimal,                  // Platform fee (e.g., 0.01 = 1%)
    pub min_confirmation_blocks: u64,       // Minimum blocks for confirmation
    pub retry_attempts: u32,                // Number of retry attempts for failed transactions
    pub retry_delay_secs: u64,              // Delay between retries
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            fee_rate: Decimal::from_str("0.01").unwrap(), // 1% platform fee
            min_confirmation_blocks: 32,                   // ~13 seconds on Solana
            retry_attempts: 3,
            retry_delay_secs: 5,
        }
    }
}

/// Settlement service for blockchain transaction execution
#[derive(Clone)]
pub struct SettlementService {
    db: PgPool,
    blockchain: BlockchainService,
    config: SettlementConfig,
    pending_settlements: Arc<RwLock<Vec<Uuid>>>,
}

impl SettlementService {
    pub fn new(db: PgPool, blockchain: BlockchainService) -> Self {
        Self::with_config(db, blockchain, SettlementConfig::default())
    }

    pub fn with_config(
        db: PgPool,
        blockchain: BlockchainService,
        config: SettlementConfig,
    ) -> Self {
        Self {
            db,
            blockchain,
            config,
            pending_settlements: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create settlement records from matched trades
    pub async fn create_settlements_from_trades(
        &self,
        trades: Vec<TradeMatch>,
    ) -> Result<Vec<Settlement>, ApiError> {
        let mut settlements = Vec::new();

        for trade in trades {
            let settlement = self.create_settlement(&trade).await?;
            settlements.push(settlement);
        }

        Ok(settlements)
    }

    /// Create a single settlement from a trade match
    async fn create_settlement(&self, trade: &TradeMatch) -> Result<Settlement, ApiError> {
        // Calculate settlement amounts
        let total_value = trade.quantity * trade.price;
        let fee_amount = total_value * self.config.fee_rate;
        let net_amount = total_value - fee_amount;

        let settlement = Settlement {
            id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(), // Would come from trade.id if available
            buyer_id: trade.buyer_id,
            seller_id: trade.seller_id,
            energy_amount: trade.quantity,
            price: trade.price,
            total_value,
            fee_amount,
            net_amount,
            status: SettlementStatus::Pending,
            blockchain_tx: None,
            created_at: Utc::now(),
            confirmed_at: None,
        };

        // Save to database
        sqlx::query(
            r#"
            INSERT INTO settlements (
                id, buyer_id, seller_id, energy_amount,
                price_per_kwh, total_amount, fee_amount, net_amount,
                status, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(settlement.id)
        .bind(settlement.buyer_id)
        .bind(settlement.seller_id)
        .bind(settlement.energy_amount.to_string())
        .bind(settlement.price.to_string())
        .bind(settlement.total_value.to_string())
        .bind(settlement.fee_amount.to_string())
        .bind(settlement.net_amount.to_string())
        .bind(settlement.status.to_string())
        .bind(settlement.created_at)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        info!(
            "📝 Created settlement {}: {} kWh at ${} (buyer: {}, seller: {})",
            settlement.id, settlement.energy_amount, settlement.price,
            settlement.buyer_id, settlement.seller_id
        );

        Ok(settlement)
    }

    /// Execute blockchain settlement for a trade
    pub async fn execute_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<SettlementTransaction, ApiError> {
        // Update status to processing
        self.update_settlement_status(settlement_id, SettlementStatus::Processing)
            .await?;

        // Get settlement details
        let settlement = self.get_settlement(settlement_id).await?;

        // Execute blockchain transaction
        match self.execute_blockchain_transfer(&settlement).await {
            Ok(tx_result) => {
                // Update settlement with transaction signature
                self.update_settlement_confirmed(
                    settlement_id,
                    &tx_result.signature,
                    SettlementStatus::Confirmed,
                )
                .await?;

                info!(
                    "✅ Settlement {} confirmed: tx {}",
                    settlement_id, tx_result.signature
                );

                Ok(tx_result)
            }
            Err(e) => {
                error!("❌ Settlement {} failed: {}", settlement_id, e);

                // Update status to failed
                self.update_settlement_status(settlement_id, SettlementStatus::Failed)
                    .await?;

                Err(ApiError::Internal(format!(
                    "Settlement execution failed: {}",
                    e
                )))
            }
        }
    }

    /// Execute the actual blockchain transfer
    async fn execute_blockchain_transfer(
        &self,
        settlement: &Settlement,
    ) -> Result<SettlementTransaction, ApiError> {
        // Note: This is a simplified implementation
        // In production, you would:
        // 1. Get buyer and seller token accounts
        // 2. Create SPL token transfer instruction
        // 3. Sign and send transaction
        // 4. Wait for confirmation

        info!(
            "🔗 Executing blockchain transfer for settlement {}",
            settlement.id
        );

        // For now, we'll simulate a successful transaction
        // In production, replace with actual Solana SPL token transfer
        let simulated_signature = format!(
            "{}{}",
            settlement.id.to_string().replace("-", ""),
            "1234567890abcdef"
        )[..64]
            .to_string();

        // Simulate network delay
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(SettlementTransaction {
            settlement_id: settlement.id,
            signature: simulated_signature,
            slot: 12345678,
            confirmation_status: "finalized".to_string(),
        })
    }

    /// Process all pending settlements
    pub async fn process_pending_settlements(&self) -> Result<usize, ApiError> {
        let pending_ids = self.get_pending_settlements().await?;

        if pending_ids.is_empty() {
            debug!("No pending settlements to process");
            return Ok(0);
        }

        info!("Processing {} pending settlements", pending_ids.len());
        let total_count = pending_ids.len();
        let mut processed = 0;

        for settlement_id in pending_ids {
            match self.execute_settlement(settlement_id).await {
                Ok(_) => {
                    processed += 1;
                }
                Err(e) => {
                    warn!(
                        "Failed to process settlement {}: {}",
                        settlement_id, e
                    );
                }
            }

            // Small delay between settlements to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("✅ Processed {}/{} settlements", processed, total_count);
        Ok(processed)
    }

    /// Get settlement by ID
    async fn get_settlement(&self, id: Uuid) -> Result<Settlement, ApiError> {
        use sqlx::Row;
        
        let row = sqlx::query(
            r#"
            SELECT 
                id, buyer_id, seller_id, energy_amount,
                price_per_kwh, total_amount, fee_amount, net_amount,
                status, blockchain_tx, created_at, confirmed_at
            FROM settlements
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound("Settlement not found".into()))?;

        let status_str: String = row.get("status");
        let status = match status_str.as_str() {
            "Pending" => SettlementStatus::Pending,
            "Processing" => SettlementStatus::Processing,
            "Confirmed" => SettlementStatus::Confirmed,
            "Failed" => SettlementStatus::Failed,
            "Cancelled" => SettlementStatus::Cancelled,
            _ => SettlementStatus::Pending,
        };

        Ok(Settlement {
            id: row.get("id"),
            trade_id: Uuid::new_v4(), // Not stored in this simplified version
            buyer_id: row.get("buyer_id"),
            seller_id: row.get("seller_id"),
            energy_amount: Decimal::from_str(&row.get::<String, _>("energy_amount"))
                .unwrap_or(Decimal::ZERO),
            price: Decimal::from_str(&row.get::<String, _>("price_per_kwh"))
                .unwrap_or(Decimal::ZERO),
            total_value: Decimal::from_str(&row.get::<String, _>("total_amount"))
                .unwrap_or(Decimal::ZERO),
            fee_amount: Decimal::from_str(&row.get::<String, _>("fee_amount"))
                .unwrap_or(Decimal::ZERO),
            net_amount: Decimal::from_str(&row.get::<String, _>("net_amount"))
                .unwrap_or(Decimal::ZERO),
            status,
            blockchain_tx: row.get("blockchain_tx"),
            created_at: row.get("created_at"),
            confirmed_at: row.get("confirmed_at"),
        })
    }

    /// Get all pending settlements
    async fn get_pending_settlements(&self) -> Result<Vec<Uuid>, ApiError> {
        use sqlx::Row;
        
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM settlements
            WHERE status = 'Pending'
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    /// Update settlement status
    async fn update_settlement_status(
        &self,
        id: Uuid,
        status: SettlementStatus,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(status.to_string())
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(())
    }

    /// Update settlement with confirmation
    async fn update_settlement_confirmed(
        &self,
        id: Uuid,
        tx_signature: &str,
        status: SettlementStatus,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET status = $1,
                blockchain_tx = $2,
                confirmed_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(status.to_string())
        .bind(tx_signature)
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(())
    }

    /// Get settlement statistics
    pub async fn get_settlement_stats(&self) -> Result<SettlementStats, ApiError> {
        use sqlx::Row;
        
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) FILTER (WHERE status = 'Pending') as pending_count,
                COUNT(*) FILTER (WHERE status = 'Processing') as processing_count,
                COUNT(*) FILTER (WHERE status = 'Confirmed') as confirmed_count,
                COUNT(*) FILTER (WHERE status = 'Failed') as failed_count,
                COALESCE(SUM(CASE WHEN status = 'Confirmed' THEN total_amount::numeric ELSE 0 END), 0) as total_settled_value
            FROM settlements
            WHERE created_at > NOW() - INTERVAL '24 hours'
            "#,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(SettlementStats {
            pending_count: row.get::<i64, _>("pending_count"),
            processing_count: row.get::<i64, _>("processing_count"),
            confirmed_count: row.get::<i64, _>("confirmed_count"),
            failed_count: row.get::<i64, _>("failed_count"),
            total_settled_value: Decimal::from_str(&row.get::<String, _>("total_settled_value"))
                .unwrap_or(Decimal::ZERO),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settlement_config_default() {
        let config = SettlementConfig::default();
        assert_eq!(config.fee_rate, Decimal::from_str("0.01").unwrap());
        assert_eq!(config.min_confirmation_blocks, 32);
    }

    #[test]
    fn test_settlement_status_display() {
        assert_eq!(SettlementStatus::Pending.to_string(), "Pending");
        assert_eq!(SettlementStatus::Confirmed.to_string(), "Confirmed");
    }

    #[test]
    fn test_settlement_creation() {
        let settlement = Settlement {
            id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(),
            buyer_id: Uuid::new_v4(),
            seller_id: Uuid::new_v4(),
            energy_amount: Decimal::from(100),
            price: Decimal::from_str("0.15").unwrap(),
            total_value: Decimal::from_str("15.00").unwrap(),
            fee_amount: Decimal::from_str("0.15").unwrap(),
            net_amount: Decimal::from_str("14.85").unwrap(),
            status: SettlementStatus::Pending,
            blockchain_tx: None,
            created_at: Utc::now(),
            confirmed_at: None,
        };

        assert_eq!(settlement.status, SettlementStatus::Pending);
        assert!(settlement.blockchain_tx.is_none());
        assert!(settlement.confirmed_at.is_none());
    }

    #[test]
    fn test_fee_calculation() {
        let config = SettlementConfig {
            fee_rate: Decimal::from_str("0.01").unwrap(), // 1%
            min_confirmation_blocks: 32,
            retry_attempts: 3,
            retry_delay_secs: 5,
        };

        let trade_amount = Decimal::from(100);
        let expected_fee = Decimal::from_str("1.00").unwrap();
        
        assert_eq!(config.fee_rate * trade_amount, expected_fee);
    }

    #[test]
    fn test_settlement_transaction_structure() {
        let tx = SettlementTransaction {
            settlement_id: Uuid::new_v4(),
            signature: "5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3".to_string(),
            slot: 12345678,
            confirmation_status: "confirmed".to_string(),
        };

        assert_eq!(tx.slot, 12345678);
        assert_eq!(tx.confirmation_status, "confirmed");
    }

    #[test]
    fn test_settlement_status_transitions() {
        // Valid transition: Pending -> Processing
        let status1 = SettlementStatus::Processing;
        assert_eq!(status1, SettlementStatus::Processing);

        // Valid transition: Processing -> Confirmed
        let status2 = SettlementStatus::Confirmed;
        assert_eq!(status2, SettlementStatus::Confirmed);
        
        // Failed state
        let status3 = SettlementStatus::Failed;
        assert_eq!(status3, SettlementStatus::Failed);
    }

    #[test]
    fn test_settlement_status_failed() {
        let status = SettlementStatus::Failed;
        assert_eq!(status.to_string(), "Failed");
    }

    #[test]
    fn test_custom_fee_rate() {
        let custom_config = SettlementConfig {
            fee_rate: Decimal::from_str("0.005").unwrap(), // 0.5%
            min_confirmation_blocks: 64,
            retry_attempts: 5,
            retry_delay_secs: 10,
        };

        assert_eq!(custom_config.fee_rate, Decimal::from_str("0.005").unwrap());
        assert_eq!(custom_config.min_confirmation_blocks, 64);
        assert_eq!(custom_config.retry_attempts, 5);
        assert_eq!(custom_config.retry_delay_secs, 10);
    }

    #[test]
    fn test_zero_fee_rate() {
        let zero_fee_config = SettlementConfig {
            fee_rate: Decimal::ZERO,
            min_confirmation_blocks: 1,
            retry_attempts: 1,
            retry_delay_secs: 1,
        };

        let trade_amount = Decimal::from(100);
        assert_eq!(zero_fee_config.fee_rate * trade_amount, Decimal::ZERO);
    }
}
