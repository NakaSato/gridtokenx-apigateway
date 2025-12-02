// Transaction Coordinator Service
// Provides unified transaction tracking by routing to existing services

use anyhow::Result;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::transaction::{
    BlockchainOperation, TransactionFilters, TransactionMonitoringConfig, TransactionResponse,
    TransactionRetryRequest, TransactionRetryResponse, TransactionStats, TransactionStatus,
    TransactionType,
};
use crate::services::BlockchainService;
use crate::services::settlement_service::SettlementService;
use crate::services::validation::TransactionValidationService;

/// Transaction Coordinator for unified tracking and monitoring
#[derive(Clone)]
pub struct TransactionCoordinator {
    db: PgPool,
    blockchain_service: Arc<BlockchainService>,
    settlement_service: Arc<SettlementService>,
    #[allow(dead_code)]
    validation_service: Arc<TransactionValidationService>,
    config: TransactionMonitoringConfig,
}

impl TransactionCoordinator {
    /// Create a new transaction coordinator
    pub fn new(
        db: PgPool,
        blockchain_service: Arc<BlockchainService>,
        settlement_service: Arc<SettlementService>,
        validation_service: Arc<TransactionValidationService>,
    ) -> Self {
        Self::with_config(
            db,
            blockchain_service,
            settlement_service,
            validation_service,
            TransactionMonitoringConfig::default(),
        )
    }

    /// Create a transaction coordinator with custom configuration
    pub fn with_config(
        db: PgPool,
        blockchain_service: Arc<BlockchainService>,
        settlement_service: Arc<SettlementService>,
        validation_service: Arc<TransactionValidationService>,
        config: TransactionMonitoringConfig,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            settlement_service,
            validation_service,
            config,
        }
    }

    /// Get transaction status by operation ID
    pub async fn get_transaction_status(
        &self,
        operation_id: Uuid,
    ) -> Result<TransactionResponse, ApiError> {
        let operation = self.get_blockchain_operation(operation_id).await?;

        Ok(TransactionResponse {
            transaction_type: operation.operation_type,
            operation_id: operation.operation_id,
            user_id: operation.user_id,
            status: operation.status,
            signature: operation.signature,
            attempts: operation.attempts,
            last_error: operation.last_error,
            created_at: operation.created_at,
            submitted_at: operation.submitted_at,
            confirmed_at: operation.confirmed_at,
            settled_at: None,
        })
    }

    /// Get transactions for a specific user
    pub async fn get_user_transactions(
        &self,
        user_id: Uuid,
        filters: TransactionFilters,
    ) -> Result<Vec<TransactionResponse>, ApiError> {
        let mut user_filters = filters;
        user_filters.user_id = Some(user_id);
        self.get_transactions(user_filters).await
    }

    /// Get transactions with filters
    pub async fn get_transactions(
        &self,
        filters: TransactionFilters,
    ) -> Result<Vec<TransactionResponse>, ApiError> {
        // Build base query
        let mut query = String::from(
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
            WHERE 1=1
            "#,
        );

        // Add filters
        // Add filters using string concatenation for simplicity
        if let Some(operation_type) = &filters.operation_type {
            query.push_str(&format!(" AND operation_type = '{}'", operation_type));
        }

        if let Some(tx_type) = &filters.tx_type {
            query.push_str(&format!(" AND tx_type = '{}'", tx_type.to_string()));
        }

        if let Some(status) = &filters.status {
            query.push_str(&format!(" AND operation_status = '{}'", status.to_string()));
        }

        if let Some(user_id) = &filters.user_id {
            query.push_str(&format!(" AND user_id = '{}'", user_id));
        }

        if let Some(date_from) = &filters.date_from {
            query.push_str(&format!(" AND created_at >= '{}'", date_from));
        }

        if let Some(date_to) = &filters.date_to {
            query.push_str(&format!(" AND created_at <= '{}'", date_to));
        }

        if let Some(min_attempts) = filters.min_attempts {
            query.push_str(&format!(" AND attempts >= {}", min_attempts));
        }

        if let Some(has_signature) = filters.has_signature {
            if has_signature {
                query.push_str(" AND signature IS NOT NULL");
            } else {
                query.push_str(" AND signature IS NULL");
            }
        }

        // Add ordering
        query.push_str(" ORDER BY created_at DESC");

        // Add limit and offset
        if let Some(limit) = filters.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filters.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        // For now, we'll use a simplified approach without dynamic parameter binding
        // This is a limitation with sqlx macros, but sufficient for our use case
        let operations = self.get_transactions_with_filters(filters).await?;

        // Convert to TransactionResponse objects
        let mut responses = Vec::new();
        for operation in operations {
            responses.push(TransactionResponse {
                transaction_type: operation.operation_type,
                operation_id: operation.operation_id,
                user_id: operation.user_id,
                status: operation.status,
                signature: operation.signature,
                attempts: operation.attempts,
                last_error: operation.last_error,
                created_at: operation.created_at,
                submitted_at: operation.submitted_at,
                confirmed_at: operation.confirmed_at,
                settled_at: None,
            });
        }

        Ok(responses)
    }

    /// Get transaction statistics
    pub async fn get_transaction_stats(&self) -> Result<TransactionStats, ApiError> {
        // Get total count
        let total_count: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM blockchain_operations")
                .fetch_one(&self.db)
                .await
                .map_err(|e| ApiError::Database(e))?;

        // Get counts by status
        let pending_count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM blockchain_operations WHERE operation_status = 'pending'",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let submitted_count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM blockchain_operations WHERE operation_status = 'submitted'",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let confirmed_count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM blockchain_operations WHERE operation_status = 'confirmed'",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let failed_count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM blockchain_operations WHERE operation_status = 'failed'",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let avg_seconds = if confirmed_count > 0 {
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(EXTRACT(EPOCH FROM (confirmed_at - created_at))) FROM blockchain_operations WHERE operation_status = 'confirmed'",
            )
            .fetch_one(&self.db)
            .await?
        } else {
            None
        };

        // Calculate success rate
        let success_rate = if total_count > 0 {
            (confirmed_count as f64) / (total_count as f64)
        } else {
            0.0
        };

        // Calculate processing count (submitted but not confirmed)
        let processing_count = submitted_count;

        Ok(TransactionStats {
            total_count,
            pending_count,
            submitted_count,
            confirmed_count,
            failed_count,
            settled_count: 0, // Add settled_count tracking if needed
            processing_count,
            avg_confirmation_time_seconds: avg_seconds,
            success_rate,
        })
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
                let table_name = match operation.operation_type {
                    TransactionType::EnergyTrade => "energy_trades",
                    TransactionType::TokenMint => "token_mints",
                    TransactionType::TokenTransfer => "token_transfers",
                    TransactionType::GovernanceVote => "governance_votes",
                    TransactionType::OracleUpdate => "oracle_updates",
                    TransactionType::RegistryUpdate => "registry_updates",
                    TransactionType::Swap => "swap_transactions",
                };
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

                            let table_name = match operation.operation_type {
                                TransactionType::EnergyTrade => "energy_trades",
                                TransactionType::TokenMint => "token_mints",
                                TransactionType::TokenTransfer => "token_transfers",
                                TransactionType::GovernanceVote => "governance_votes",
                                TransactionType::OracleUpdate => "oracle_updates",
                                TransactionType::RegistryUpdate => "registry_updates",
                                TransactionType::Swap => "swap_transactions",
                            };
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

                            let table_name = match operation.operation_type {
                                TransactionType::EnergyTrade => "energy_trades",
                                TransactionType::TokenMint => "token_mints",
                                TransactionType::TokenTransfer => "token_transfers",
                                TransactionType::GovernanceVote => "governance_votes",
                                TransactionType::OracleUpdate => "oracle_updates",
                                TransactionType::RegistryUpdate => "registry_updates",
                                TransactionType::Swap => "swap_transactions",
                            };
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

    /// Retry failed transactions
    pub async fn retry_failed_transactions(&self, max_attempts: i32) -> Result<usize, ApiError> {
        if !self.config.enabled {
            debug!("Transaction retry is disabled");
            return Ok(0);
        }

        // Get failed transactions with attempts less than max_attempts
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
            WHERE operation_status = 'failed'
            AND attempts < $1
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )
        .bind(max_attempts)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        let failed_operations: Result<Vec<BlockchainOperation>, sqlx::Error> = rows
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
                        .unwrap_or(TransactionStatus::Failed),
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

        let failed_operations = failed_operations.map_err(ApiError::Database)?;

        let mut retried_count = 0;

        for operation in failed_operations {
            info!(
                "Retrying transaction {} ({}) after {} attempts",
                operation.operation_id, operation.operation_type, operation.attempts
            );

            // Route to appropriate service based on operation type
            match operation.operation_type.as_str() {
                "settlement" => {
                    if let Err(e) = self
                        .retry_settlement_transaction(operation.operation_id)
                        .await
                    {
                        error!(
                            "Failed to retry settlement transaction {}: {}",
                            operation.operation_id, e
                        );
                        continue;
                    }
                }
                // Add retry logic for other operation types as needed
                _ => {
                    error!(
                        "No retry handler for operation type: {}",
                        operation.operation_type
                    );
                    continue;
                }
            }

            retried_count += 1;
        }

        Ok(retried_count)
    }

    /// Retry a specific transaction
    pub async fn retry_transaction(
        &self,
        request: TransactionRetryRequest,
    ) -> Result<TransactionRetryResponse, ApiError> {
        // Get current operation status
        let operation = match self.get_blockchain_operation(request.operation_id).await {
            Ok(op) => op,
            Err(e) => {
                return Ok(TransactionRetryResponse {
                    success: false,
                    attempts: 0,
                    last_error: Some(format!("Failed to get transaction: {}", e)),
                    signature: None,
                    status: TransactionStatus::Failed,
                });
            }
        };

        // Clone needed values before moving
        let op_type = operation.operation_type.clone();
        let op_attempts = operation.attempts;
        let op_sig = operation.signature.clone();
        let op_status = operation.status.clone();

        // Verify operation type matches if specified
        if let Some(ref requested_type) = request.operation_type {
            if op_type.as_str() != requested_type {
                return Ok(TransactionRetryResponse {
                    success: false,
                    attempts: op_attempts,
                    last_error: Some("Operation type mismatch".to_string()),
                    signature: op_sig.clone(),
                    status: op_status,
                });
            }
        }

        // Check if retry attempts exceeded
        let max_attempts = request
            .max_attempts
            .unwrap_or(self.config.max_retry_attempts);
        if op_attempts >= max_attempts {
            return Ok(TransactionRetryResponse {
                success: false,
                attempts: op_attempts,
                last_error: Some("Maximum retry attempts exceeded".to_string()),
                signature: op_sig,
                status: op_status,
            });
        }

        // Clone needed values before moving
        let op_id = operation.operation_id;
        let op_type = operation.operation_type.clone();
        let op_attempts = operation.attempts;
        let op_sig = operation.signature.clone();
        let op_status = operation.status.clone();

        // Route to appropriate service based on operation type
        match op_type.as_str() {
            "settlement" => {
                self.retry_settlement_transaction(op_id).await?;
            }
            // Add retry logic for other operation types as needed
            _ => {
                return Ok(TransactionRetryResponse {
                    success: false,
                    attempts: op_attempts,
                    last_error: Some(format!("No retry handler for operation type: {}", op_type)),
                    signature: op_sig.clone(),
                    status: op_status,
                });
            }
        }

        // Get updated operation status
        let updated_operation = self
            .get_blockchain_operation(operation.operation_id)
            .await?;

        // Clone needed values before moving
        let updated_attempts = updated_operation.attempts;
        let updated_last_error = updated_operation.last_error.clone();
        let updated_sig = updated_operation.signature.clone();
        let updated_status = updated_operation.status.clone();

        Ok(TransactionRetryResponse {
            success: true,
            attempts: updated_attempts,
            last_error: updated_last_error,
            signature: updated_sig,
            status: updated_status,
        })
    }

    /// Helper method to get blockchain operation by ID
    async fn get_blockchain_operation(
        &self,
        operation_id: Uuid,
    ) -> Result<BlockchainOperation, ApiError> {
        // Query from blockchain_operations view
        let row = sqlx::query(
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
            WHERE operation_id = $1
            "#,
        )
        .bind(operation_id)
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(BlockchainOperation {
            operation_type: row.get("operation_type"),
            operation_id: row.get("operation_id"),
            user_id: row.get("user_id"),
            signature: row.get("signature"),
            tx_type: row.get("tx_type"),
            status: row
                .get::<Option<String>, _>("operation_status")
                .and_then(|s| s.parse().ok())
                .unwrap_or(TransactionStatus::Pending),
            operation_status: row.get("operation_status"),
            attempts: row.get::<Option<i32>, _>("attempts").unwrap_or(0),
            last_error: row.get("last_error"),
            payload: serde_json::Value::Null,
            max_priority_fee: None,
            submitted_at: row.get("submitted_at"),
            confirmed_at: row.get("confirmed_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Mark transaction as confirmed
    async fn mark_transaction_confirmed(
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
    async fn mark_transaction_failed(
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

    /// Retry a settlement transaction
    async fn retry_settlement_transaction(&self, settlement_id: Uuid) -> Result<(), ApiError> {
        // Increment attempt count first
        let result: Option<bool> =
            sqlx::query_scalar("SELECT increment_blockchain_attempts($1, $2, $3)")
                .bind("settlements")
                .bind(settlement_id)
                .bind("Retrying settlement")
                .fetch_one(&self.db)
                .await
                .map_err(ApiError::Database)?;

        if !result.unwrap_or(false) {
            error!(
                "Failed to increment attempt count for settlement {}",
                settlement_id
            );
        }

        // Use the settlement service to retry
        // This is a simplified approach - in a real implementation,
        // you would have a dedicated retry method in the settlement service
        match self
            .settlement_service
            .execute_settlement(settlement_id)
            .await
        {
            Ok(transaction) => {
                // Mark as submitted
                let result: Option<bool> =
                    sqlx::query_scalar("SELECT mark_blockchain_submitted($1, $2, $3, $4)")
                        .bind("settlements")
                        .bind(settlement_id)
                        .bind(&transaction.signature)
                        .bind("settlement")
                        .fetch_one(&self.db)
                        .await
                        .map_err(ApiError::Database)?;

                if !result.unwrap_or(false) {
                    error!("Failed to mark settlement {} as submitted", settlement_id);
                }

                info!(
                    "Settlement {} retry submitted with signature: {}",
                    settlement_id, transaction.signature
                );
            }
            Err(e) => {
                // Mark as failed
                self.mark_transaction_failed("settlements", settlement_id, Some(&e.to_string()))
                    .await?;
                error!("Failed to retry settlement {}: {}", settlement_id, e);
            }
        }

        Ok(())
    }

    /// Helper method to get transactions with filters
    async fn get_transactions_with_filters(
        &self,
        filters: TransactionFilters,
    ) -> Result<Vec<BlockchainOperation>, ApiError> {
        // Check if blockchain_operations view exists, if not create a fallback query
        let view_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'blockchain_operations')"
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let mut query = if view_exists {
            // Use the unified view if it exists
            String::from(
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
                WHERE 1=1
                "#,
            )
        } else {
            // Fallback to direct table queries if view doesn't exist
            String::from(
                r#"
                SELECT
                    'trading_order' AS operation_type,
                    id AS operation_id,
                    user_id,
                    transaction_hash AS signature,
                    'energy_trade' AS tx_type,
                    status::text AS operation_status,
                    0 AS attempts,
                    NULL AS last_error,
                    created_at AS submitted_at,
                    settled_at AS confirmed_at,
                    created_at,
                    updated_at
                FROM trading_orders
                WHERE transaction_hash IS NOT NULL
                "#,
            )
        };

        // Add filters
        if let Some(operation_type) = &filters.operation_type {
            query.push_str(&format!(" AND operation_type = '{}'", operation_type));
        }

        if let Some(tx_type) = &filters.tx_type {
            query.push_str(&format!(" AND tx_type = '{}'", tx_type.to_string()));
        }

        if let Some(status) = &filters.status {
            query.push_str(&format!(" AND operation_status = '{}'", status.to_string()));
        }

        if let Some(user_id) = &filters.user_id {
            query.push_str(&format!(" AND user_id = '{}'", user_id));
        }

        if let Some(date_from) = &filters.date_from {
            query.push_str(&format!(" AND created_at >= '{}'", date_from));
        }

        if let Some(date_to) = &filters.date_to {
            query.push_str(&format!(" AND created_at <= '{}'", date_to));
        }

        if let Some(min_attempts) = filters.min_attempts {
            query.push_str(&format!(" AND attempts >= {}", min_attempts));
        }

        if let Some(has_signature) = filters.has_signature {
            if has_signature {
                query.push_str(" AND signature IS NOT NULL");
            } else {
                query.push_str(" AND signature IS NULL");
            }
        }

        // Add ordering
        query.push_str(" ORDER BY created_at DESC");

        // Add limit and offset
        if let Some(limit) = filters.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filters.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        // Execute query
        let rows = sqlx::query(&query)
            .fetch_all(&self.db)
            .await
            .map_err(ApiError::Database)?;

        // Convert to BlockchainOperation objects
        let operations: Result<Vec<BlockchainOperation>, sqlx::Error> = rows
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

        let operations = operations.map_err(ApiError::Database)?;

        Ok(operations)
    }
}
