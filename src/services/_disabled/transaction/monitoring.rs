use anyhow::Result;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::transaction::{
    BlockchainOperation, TransactionMonitoringConfig, TransactionStatus, TransactionType,
};
use crate::services::BlockchainService;

/// Service for monitoring transaction status
#[derive(Clone)]
pub struct TransactionMonitorService {
    db: PgPool,
    blockchain_service: Arc<BlockchainService>,
    config: TransactionMonitoringConfig,
}

impl TransactionMonitorService {
    pub fn new(
        db: PgPool,
        blockchain_service: Arc<BlockchainService>,
        config: TransactionMonitoringConfig,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            config,
        }
    }

    /// Monitor pending transactions and update their status
    pub async fn monitor_pending_transactions(&self) -> Result<usize, ApiError> {
        if !self.config.enabled {
            debug!("Transaction monitoring is disabled");
            return Ok(0);
        }

        // Get pending and submitted transactions
        let rows = sqlx::query(
            r#"
            SELECT
                operation_type,
                operation_id,
                user_id,
                signature,
                tx_type,
                operation_status,
                attempts,
                last_error,
                submitted_at,
                confirmed_at,
                created_at,
                updated_at
            FROM blockchain_operations
            WHERE operation_status IN ('pending', 'submitted')
            AND signature IS NOT NULL
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        let pending_operations: Result<Vec<BlockchainOperation>, sqlx::Error> = rows
            .into_iter()
            .map(|row| {
                Ok(BlockchainOperation {
                    operation_type: row.try_get("operation_type")?,
                    operation_id: row.try_get("operation_id")?,
                    user_id: row.try_get("user_id")?,
                    signature: row.try_get("signature")?,
                    tx_type: row.try_get("tx_type")?,
                    status: row
                        .try_get::<Option<String>, _>("operation_status")?
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(TransactionStatus::Pending),
                    operation_status: row.try_get("operation_status")?,
                    attempts: row.try_get::<Option<i32>, _>("attempts")?.unwrap_or(0),
                    last_error: row.try_get("last_error")?,
                    payload: serde_json::Value::Null,
                    max_priority_fee: None,
                    submitted_at: row.try_get("submitted_at")?,
                    confirmed_at: row.try_get("confirmed_at")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect();

        let pending_operations = pending_operations.map_err(ApiError::Database)?;

        let mut updated_count = 0;

        for operation in pending_operations {
            // Check if the transaction has been pending for too long
            let now = Utc::now();
            let pending_duration = if let Some(submitted_at) = operation.submitted_at {
                now.signed_duration_since(submitted_at).num_seconds()
            } else {
                now.signed_duration_since(operation.created_at)
                    .num_seconds()
            };

            if pending_duration > self.config.transaction_expiry_seconds as i64 {
                warn!(
                    "Transaction {} ({}) has been pending for {} seconds, marking as failed",
                    operation.operation_id, operation.operation_type, pending_duration
                );

                // Mark as failed - determine table name from operation type
                let table_name = self.get_table_name(&operation.operation_type);
                self.mark_transaction_failed(
                    table_name,
                    operation.operation_id,
                    Some("Transaction pending too long"),
                )
                .await?;

                updated_count += 1;
                continue;
            }

            // Check transaction status if it has been submitted
            if operation.status == TransactionStatus::Submitted && operation.signature.is_some() {
                if let Some(signature) = &operation.signature {
                    // Parse signature
                    let signature = match solana_sdk::signature::Signature::from_str(signature) {
                        Ok(sig) => sig,
                        Err(e) => {
                            error!(
                                "Invalid signature format for transaction {}: {}",
                                operation.operation_id, e
                            );
                            continue;
                        }
                    };

                    // Check signature status
                    match self
                        .blockchain_service
                        .get_signature_status(&signature)
                        .await
                    {
                        Ok(Some(true)) => {
                            // Transaction is confirmed
                            info!(
                                "Transaction {} ({}) confirmed",
                                operation.operation_id, operation.operation_type
                            );

                            let table_name = self.get_table_name(&operation.operation_type);
                            if self
                                .mark_transaction_confirmed(
                                    table_name,
                                    operation.operation_id,
                                    signature,
                                )
                                .await?
                            {
                                updated_count += 1;
                            }
                        }
                        Ok(Some(false)) => {
                            // Transaction failed
                            warn!(
                                "Transaction {} ({}) failed",
                                operation.operation_id, operation.operation_type
                            );

                            let table_name = self.get_table_name(&operation.operation_type);
                            if self
                                .mark_transaction_failed(
                                    table_name,
                                    operation.operation_id,
                                    Some("Transaction failed on blockchain"),
                                )
                                .await?
                            {
                                updated_count += 1;
                            }
                        }
                        Ok(None) => {
                            // Transaction not yet confirmed
                            debug!(
                                "Transaction {} ({}) still pending confirmation",
                                operation.operation_id, operation.operation_type
                            );
                        }
                        Err(e) => {
                            error!(
                                "Error checking status for transaction {}: {}",
                                operation.operation_id, e
                            );
                        }
                    }
                }
            }
        }

        Ok(updated_count)
    }

    fn get_table_name(&self, operation_type: &TransactionType) -> &'static str {
        match operation_type {
            TransactionType::EnergyTrade => "energy_trades",
            TransactionType::TokenMint => "token_mints",
            TransactionType::TokenTransfer => "token_transfers",
            TransactionType::GovernanceVote => "governance_votes",
            TransactionType::OracleUpdate => "oracle_updates",
            TransactionType::RegistryUpdate => "registry_updates",
            TransactionType::Swap => "swap_transactions",
        }
    }

    /// Mark transaction as confirmed
    pub async fn mark_transaction_confirmed(
        &self,
        table_name: &str,
        record_id: Uuid,
        signature: solana_sdk::signature::Signature,
    ) -> Result<bool, ApiError> {
        // Use helper function from the migration
        let result: Option<bool> =
            sqlx::query_scalar("SELECT mark_blockchain_confirmed($1, $2, $3, 'confirmed')")
                .bind(table_name)
                .bind(record_id)
                .bind(signature.to_string())
                .fetch_one(&self.db)
                .await
                .map_err(ApiError::Database)?;

        Ok(result.unwrap_or(false))
    }

    /// Mark transaction as failed
    pub async fn mark_transaction_failed(
        &self,
        table_name: &str,
        record_id: Uuid,
        error_message: Option<&str>,
    ) -> Result<bool, ApiError> {
        // Use helper function from the migration
        let result: Option<bool> =
            sqlx::query_scalar("SELECT increment_blockchain_attempts($1, $2, $3)")
                .bind(table_name)
                .bind(record_id)
                .bind(error_message)
                .fetch_one(&self.db)
                .await
                .map_err(ApiError::Database)?;

        // Now update status to failed
        let query = format!(
            "UPDATE {} SET blockchain_status = 'failed', updated_at = NOW() WHERE id = $1",
            table_name
        );

        sqlx::query(&query)
            .bind(record_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::Database)?;

        Ok(result.unwrap_or(false))
    }
}
