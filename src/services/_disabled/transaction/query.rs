use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::transaction::{
    BlockchainOperation, TransactionFilters, TransactionResponse, TransactionStats,
    TransactionStatus,
};

/// Service for querying transaction data
#[derive(Clone)]
pub struct TransactionQueryService {
    db: PgPool,
}

impl TransactionQueryService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
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

    /// Helper method to get transactions with filters
    async fn get_transactions_with_filters(
        &self,
        filters: TransactionFilters,
    ) -> Result<Vec<BlockchainOperation>, ApiError> {
        // Construct basic query - in a real impl we'd use the filters dynamically
        // but for now we'll just check if we have the helper view or need to construct custom queries

        let mut sql = String::from(
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

        // Note: Actual filtering already happened in string construction in get_transactions
        // but we need to re-implement or pass the query string.
        // For this refactor, let's just duplicate the logic efficiently or use the builder pattern properly?
        // To avoid code duplication and logic errors, we should really move the query building HERE
        // and have `get_transactions` call this.
        // BUT `get_transactions` builds a string `query` but then IGNORES it and calls `get_transactions_with_filters`!
        // Wait, looking at original code:
        // `TransactionCoordinator.get_transactions` logic (lines 100-199) constructs `query` string
        // THEN calls `self.get_transactions_with_filters(filters)` (line 178).
        // It does NOT use the `query` string it built!
        // That seems like a bug or legacy code in the original file.
        // Let's check `get_transactions_with_filters` implementation in original file (lines 828-970).
        // It likely rebuilds the query.

        // I will implement `get_transactions_with_filters` by ACTUALLY building the query here.

        if let Some(operation_type) = &filters.operation_type {
            sql.push_str(&format!(" AND operation_type = '{}'", operation_type));
        }

        if let Some(tx_type) = &filters.tx_type {
            sql.push_str(&format!(" AND tx_type = '{}'", tx_type.to_string()));
        }

        if let Some(status) = &filters.status {
            sql.push_str(&format!(" AND operation_status = '{}'", status.to_string()));
        }

        if let Some(user_id) = &filters.user_id {
            sql.push_str(&format!(" AND user_id = '{}'", user_id));
        }

        if let Some(date_from) = &filters.date_from {
            sql.push_str(&format!(" AND created_at >= '{}'", date_from));
        }

        if let Some(date_to) = &filters.date_to {
            sql.push_str(&format!(" AND created_at <= '{}'", date_to));
        }

        if let Some(min_attempts) = filters.min_attempts {
            sql.push_str(&format!(" AND attempts >= {}", min_attempts));
        }

        // Ordering
        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filters.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let rows = sqlx::query(&sql)
            .fetch_all(&self.db)
            .await
            .map_err(ApiError::Database)?;

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

        operations.map_err(ApiError::Database)
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
            settled_count: 0,
            processing_count,
            avg_confirmation_time_seconds: avg_seconds,
            success_rate,
        })
    }

    /// Helper method to get blockchain operation by ID
    pub async fn get_blockchain_operation(
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
}
