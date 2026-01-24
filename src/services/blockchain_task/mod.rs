use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;
use rust_decimal::Decimal;

use crate::services::MarketClearingService;

#[derive(Debug, sqlx::Type, Serialize, Deserialize, Clone, PartialEq)]
#[sqlx(type_name = "blockchain_task_type", rename_all = "snake_case")]
pub enum BlockchainTaskType {
    EscrowRefund,
    Settlement,
    Minting,
}

#[derive(Debug, sqlx::Type, Serialize, Deserialize, Clone, PartialEq)]
#[sqlx(type_name = "task_status", rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    MaxRetries,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EscrowRefundPayload {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub asset_type: String,
    pub order_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum TaskPayload {
    EscrowRefund(EscrowRefundPayload),
    // Add other payloads here as needed
}

#[derive(Clone, Debug)]
pub struct BlockchainTaskService {
    db: PgPool,
    market_clearing_service: Arc<MarketClearingService>,
}

impl BlockchainTaskService {
    pub fn new(db: PgPool, market_clearing_service: Arc<MarketClearingService>) -> Self {
        Self {
            db,
            market_clearing_service,
        }
    }

    /// Queue a new blockchain task (non-blocking)
    pub async fn queue_task(
        &self,
        task_type: BlockchainTaskType,
        payload: TaskPayload,
    ) -> Result<Uuid> {
        let payload_json = serde_json::to_value(payload)?;
        
        let id = sqlx::query!(
            r#"
            INSERT INTO blockchain_tasks (task_type, payload, status, next_retry_at)
            VALUES ($1, $2, 'pending', NOW())
            RETURNING id
            "#,
            task_type.clone() as BlockchainTaskType,
            payload_json
        )
        .fetch_one(&self.db)
        .await?
        .id;

        info!("Queued new blockchain task: {} ({:?})", id, task_type);
        Ok(id)
    }

    /// Process pending tasks (called by background worker)
    pub async fn process_pending_tasks(&self) -> Result<()> {
        // Fetch tasks that are pending or failed (but not max retries) and ready for retry
        let tasks = sqlx::query!(
            r#"
            SELECT id, task_type as "task_type: BlockchainTaskType", payload, retry_count, max_retries
            FROM blockchain_tasks
            WHERE status IN ('pending', 'failed') 
              AND next_retry_at <= NOW()
              AND retry_count < max_retries
            ORDER BY next_retry_at ASC
            LIMIT 10
            FOR UPDATE SKIP LOCKED
            "#
        )
        .fetch_all(&self.db)
        .await?;

        if !tasks.is_empty() {
            info!("Found {} pending blockchain tasks to process", tasks.len());
        }

        for task in tasks {
            let task_id = task.id;
            let payload: TaskPayload = match serde_json::from_value(task.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to deserialize task payload for {}: {}", task_id, e);
                    self.mark_as_failed(task_id, &e.to_string(), false).await?;
                    continue;
                }
            };

            info!("Processing task {} (Attempt {}/{})", task_id, task.retry_count + 1, task.max_retries);

            match self.execute_task(&task.task_type, payload).await {
                Ok(_) => {
                    info!("✅ Task {} completed successfully", task_id);
                    self.mark_as_completed(task_id).await?;
                }
                Err(e) => {
                    warn!("❌ Task {} failed: {}", task_id, e);
                    self.mark_as_failed(task_id, &e.to_string(), true).await?;
                }
            }
        }

        Ok(())
    }

    async fn execute_task(&self, task_type: &BlockchainTaskType, payload: TaskPayload) -> Result<()> {
        match (task_type, payload) {
            (BlockchainTaskType::EscrowRefund, TaskPayload::EscrowRefund(data)) => {
                // Call MarketClearingService to execute refund
                // Note: We need to expose a method in MarketClearingService that is idempotent
                // stored previously in MarketClearingService but needs to be accessible here.
                // Or better, move the pure blockchain logic to BlockchainService and call it?
                // Given the current architecture, MarketClearingService holds the business logic wrapper.
                
                // We access the underlying method directly.
                // However, `MarketClearingService` methods are currently private/super-only for some parts.
                // We might need to make `execute_escrow_refund` public or `pub(crate)`.
                
                let sig = self.market_clearing_service
                    .execute_escrow_refund_retry(&data.user_id, data.amount, &data.asset_type)
                    .await?;
                
                info!("Escrow refund executed via retry queue: {}", sig);
                Ok(())
            }
            _ => Err(anyhow::anyhow!("Unsupported task type or payload mismatch")),
        }
    }

    async fn mark_as_completed(&self, task_id: Uuid) -> Result<()> {
        sqlx::query!(
            "UPDATE blockchain_tasks SET status = 'completed', updated_at = NOW() WHERE id = $1",
            task_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn mark_as_failed(&self, task_id: Uuid, error: &str, valid_for_retry: bool) -> Result<()> {
        if valid_for_retry {
            // Exponential backoff: 5s, 25s, 125s...
            sqlx::query!(
                r#"
                UPDATE blockchain_tasks 
                SET status = CASE WHEN retry_count + 1 >= max_retries THEN 'max_retries' ELSE 'failed' END::task_status,
                    retry_count = retry_count + 1,
                    last_error = $2,
                    next_retry_at = NOW() + (POWER(5, retry_count) * INTERVAL '1 second'),
                    updated_at = NOW()
                WHERE id = $1
                "#,
                task_id,
                error
            )
            .execute(&self.db)
            .await?;
        } else {
            // Irrecoverable error
            sqlx::query!(
                "UPDATE blockchain_tasks SET status = 'max_retries', last_error = $2, updated_at = NOW() WHERE id = $1",
                task_id,
                error
            )
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }
}
