//! Recurring Orders Scheduler Service
//!
//! Background service that executes recurring orders at their scheduled times

use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, error};
use chrono::{Utc, Duration as ChronoDuration};
use uuid::Uuid;

use crate::models::trading::{IntervalType, RecurringStatus};
use crate::database::schema::types::{OrderSide, OrderType, OrderStatus};

/// Recurring order scheduler configuration
#[derive(Debug, Clone)]
pub struct RecurringSchedulerConfig {
    /// How often to check for orders to execute (in seconds)
    pub check_interval_secs: u64,
    /// Whether the scheduler is enabled
    pub enabled: bool,
}

impl Default for RecurringSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
            enabled: true,
        }
    }
}

/// Recurring order scheduler service
#[derive(Clone)]
pub struct RecurringScheduler {
    db: PgPool,
    config: RecurringSchedulerConfig,
}

impl RecurringScheduler {
    pub fn new(db: PgPool, config: RecurringSchedulerConfig) -> Self {
        Self { db, config }
    }

    /// Start the scheduler loop
    pub async fn start(self: Arc<Self>) {
        if !self.config.enabled {
            info!("Recurring order scheduler is disabled");
            return;
        }

        info!("Starting recurring order scheduler with {}s interval", self.config.check_interval_secs);
        
        let mut check_interval = interval(Duration::from_secs(self.config.check_interval_secs));

        loop {
            check_interval.tick().await;
            
            if let Err(e) = self.process_due_orders().await {
                error!("Recurring scheduler error: {}", e);
            }
        }
    }

    /// Process orders that are due for execution
    pub(crate) async fn process_due_orders(&self) -> anyhow::Result<()> {
        let now = Utc::now();

        // Get orders due for execution
        let due_rows = sqlx::query(
            r#"
            SELECT id, user_id, side, energy_amount,
                   max_price_per_kwh, min_price_per_kwh,
                   interval_type,
                   interval_value,
                   total_executions,
                   max_executions, session_token
            FROM recurring_orders
            WHERE status = 'active' 
              AND next_execution_at <= $1
            ORDER BY next_execution_at ASC
            LIMIT 50
            "#,
        )
        .bind(now)
        .fetch_all(&self.db)
        .await?;

        if due_rows.is_empty() {
            return Ok(());
        }

        info!("Processing {} due recurring orders", due_rows.len());

        for row in due_rows {
            use sqlx::Row;
            let id: Uuid = row.get("id");
            let user_id: Uuid = row.get("user_id");
            let side: OrderSide = row.get("side");
            let energy_amount: Decimal = row.get("energy_amount");
            let max_price: Option<Decimal> = row.get("max_price_per_kwh");
            let min_price: Option<Decimal> = row.get("min_price_per_kwh");
            let interval_type: IntervalType = row.get("interval_type");
            let interval_value: i32 = row.get("interval_value");
            let total_executions: i32 = row.get("total_executions");
            let max_executions: Option<i32> = row.get("max_executions");
            let session_token: Option<String> = row.get("session_token");

            if let Err(e) = self.execute_order(
                id,
                user_id,
                side,
                energy_amount,
                max_price,
                min_price,
                interval_type,
                interval_value,
                total_executions,
                max_executions,
                session_token,
            ).await {
                error!("Failed to execute recurring order {}: {}", id, e);
                
                // Record failed execution
                let _ = self.record_execution(id, None, "failed", Some(&e.to_string())).await;
            }
        }

        Ok(())
    }

    /// Execute a single recurring order
    async fn execute_order(
        &self,
        recurring_id: Uuid,
        user_id: Uuid,
        side: OrderSide,
        energy_amount: Decimal,
        max_price: Option<Decimal>,
        min_price: Option<Decimal>,
        interval_type: IntervalType,
        interval_value: i32,
        total_executions: i32,
        max_executions: Option<i32>,
        session_token: Option<String>,
    ) -> anyhow::Result<()> {
        let now = Utc::now();
        
        // Begin transaction
        let mut tx = self.db.begin().await?;

        // Create trading order
        let order_id = Uuid::new_v4();
        let price = match side {
            OrderSide::Buy => max_price.unwrap_or(Decimal::ZERO),
            OrderSide::Sell => min_price.unwrap_or(Decimal::ZERO),
        };

        let order_type = if price > Decimal::ZERO {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        sqlx::query(
            r#"
            INSERT INTO trading_orders (
                id, user_id, order_type, side, energy_amount, price_per_kwh,
                filled_amount, status, created_at, expires_at, session_token
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(order_id)
        .bind(user_id)
        .bind(order_type)
        .bind(side)
        .bind(energy_amount)
        .bind(price)
        .bind(Decimal::ZERO)
        .bind(OrderStatus::Pending)
        .bind(now)
        .bind(now + ChronoDuration::hours(24))
        .bind(session_token)
        .execute(&mut *tx)
        .await?;

        // Calculate next execution time
        let next_execution = match interval_type {
            IntervalType::Hourly => now + ChronoDuration::hours(interval_value as i64),
            IntervalType::Daily => now + ChronoDuration::days(interval_value as i64),
            IntervalType::Weekly => now + ChronoDuration::weeks(interval_value as i64),
            IntervalType::Monthly => now + ChronoDuration::days(30 * interval_value as i64),
        };

        let new_total = total_executions + 1;
        
        // Check if max executions reached
        let new_status = if let Some(max) = max_executions {
            if new_total >= max {
                RecurringStatus::Completed
            } else {
                RecurringStatus::Active
            }
        } else {
            RecurringStatus::Active
        };

        // Update recurring order
        sqlx::query!(
            r#"
            UPDATE recurring_orders
            SET next_execution_at = $1,
                last_executed_at = $2,
                total_executions = $3,
                status = $4,
                updated_at = $2
            WHERE id = $5
            "#,
            next_execution,
            now,
            new_total,
            new_status as RecurringStatus,
            recurring_id
        )
        .execute(&mut *tx)
        .await?;

        // Record successful execution
        sqlx::query!(
            r#"
            INSERT INTO recurring_order_executions (
                recurring_order_id, trading_order_id, status, energy_amount, price_per_kwh
            ) VALUES ($1, $2, 'success', $3, $4)
            "#,
            recurring_id,
            order_id,
            energy_amount,
            price
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "Executed recurring order {} -> created trading order {} (execution {}/{})",
            recurring_id, order_id, new_total, max_executions.map(|m| m.to_string()).unwrap_or_else(|| "∞".to_string())
        );

        // TODO: Send WebSocket notification to user

        Ok(())
    }

    /// Record an execution attempt
    async fn record_execution(
        &self,
        recurring_id: Uuid,
        trading_order_id: Option<Uuid>,
        status: &str,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO recurring_order_executions (
                recurring_order_id, trading_order_id, status, error_message
            ) VALUES ($1, $2, $3, $4)
            "#,
            recurring_id,
            trading_order_id,
            status,
            error
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }
}
