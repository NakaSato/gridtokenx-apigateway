use anyhow::Result;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::transaction::{
    BlockchainOperation, TransactionMonitoringConfig, TransactionRetryRequest,
    TransactionRetryResponse, TransactionStatus,
};
use crate::services::settlement::SettlementService;
use crate::services::transaction::query::TransactionQueryService;

/// Service for retrying failed transactions
#[derive(Clone)]
pub struct TransactionRecoveryService {
    db: PgPool,
    settlement: Arc<SettlementService>,
    query_service: TransactionQueryService,
    config: TransactionMonitoringConfig,
}

impl TransactionRecoveryService {
    pub fn new(
        db: PgPool,
        settlement: Arc<SettlementService>,
        query_service: TransactionQueryService,
        config: TransactionMonitoringConfig,
    ) -> Self {
        Self {
            db,
            settlement,
            query_service,
            config,
        }
    }

    /// Retry failed transactions
    pub async fn retry_failed_transactions(&self, max_attempts: i32) -> Result<usize, ApiError> {
        if !self.config.enabled {
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
        let operation = match self
            .query_service
            .get_blockchain_operation(request.operation_id)
            .await
        {
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
        let op_type_inner = operation.operation_type.clone();

        // Route to appropriate service based on operation type
        match op_type_inner.as_str() {
            "settlement" => {
                self.retry_settlement_transaction(op_id).await?;
            }
            // Add retry logic for other operation types as needed
            _ => {
                return Ok(TransactionRetryResponse {
                    success: false,
                    attempts: op_attempts,
                    last_error: Some(format!(
                        "No retry handler for operation type: {}",
                        op_type_inner
                    )),
                    signature: op_sig.clone(),
                    status: op_status,
                });
            }
        }

        // Get updated operation status
        let updated_operation = self
            .query_service
            .get_blockchain_operation(operation.operation_id)
            .await?;

        // Clone needed values
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
        match self
            .settlement
            .execute_settlement(settlement_id)
            .await
        {
            Ok(_transaction) => {
                // Mark as submitted
                let result: Option<bool> =
                    sqlx::query_scalar("SELECT mark_blockchain_submitted($1, $2, $3, $4)")
                        .bind("settlements")
                        .bind(settlement_id)
                        .bind("settlement") // tx_type
                        .bind(Option::<String>::None) // signature (initially null until signed/sent?)
                        .fetch_one(&self.db)
                        .await
                        .map_err(ApiError::Database)?;

                if !result.unwrap_or(false) {
                    // Log error but don't fail, as the tx might have been submitted
                    error!("Failed to mark settlement {} as submitted", settlement_id);
                }
            }
            Err(e) => {
                // Log failure
                error!("Retry failed for settlement {}: {}", settlement_id, e);
                return Err(e);
            }
        }

        Ok(())
    }
}
