use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use uuid::Uuid;
use tracing::info;

use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::error::ApiError;
use super::MarketClearingService;
use super::types::{OrderBookEntry, Settlement};

impl MarketClearingService {
    /// Get current order book for an epoch
    pub async fn get_order_book(
        &self,
        epoch_id: Uuid,
    ) -> Result<(Vec<OrderBookEntry>, Vec<OrderBookEntry>)> {
        info!("Getting order book for epoch: {}", epoch_id);

        // Get pending buy orders (sorted by price descending, then time ascending)
        let buy_orders: Vec<OrderBookEntry> = sqlx::query_as!(
            OrderBookEntry,
            r#"
            SELECT 
                id as order_id, user_id, side as "side!: OrderSide", 
                energy_amount, price_per_kwh as "price_per_kwh!", created_at as "created_at!", zone_id
            FROM trading_orders 
            WHERE status = 'pending' AND side = 'buy' AND epoch_id = $1 AND price_per_kwh IS NOT NULL
            ORDER BY price_per_kwh DESC, created_at ASC
            "#,
            epoch_id
        )
        .fetch_all(&self.db)
        .await?;

        info!(
            "Found {} buy orders for epoch {}",
            buy_orders.len(),
            epoch_id
        );

        // Get pending sell orders (sorted by price ascending, then time ascending)
        let sell_orders: Vec<OrderBookEntry> = sqlx::query_as!(
            OrderBookEntry,
            r#"
            SELECT 
                id as order_id, user_id, side as "side!: OrderSide", 
                energy_amount, price_per_kwh as "price_per_kwh!", created_at as "created_at!", zone_id
            FROM trading_orders 
            WHERE status = 'pending' AND side = 'sell' AND epoch_id = $1 AND price_per_kwh IS NOT NULL
            ORDER BY price_per_kwh ASC, created_at ASC
            "#,
            epoch_id
        )
        .fetch_all(&self.db)
        .await?;

        info!(
            "Found {} sell orders for epoch {}",
            sell_orders.len(),
            epoch_id
        );

        Ok((buy_orders, sell_orders))
    }

    /// Create a new trading order (DB and On-Chain)
    pub async fn create_order(
        &self,
        user_id: Uuid,
        side: OrderSide,
        order_type: OrderType,
        energy_amount: Decimal,
        price_per_kwh: Option<Decimal>,
        expiry_time: Option<DateTime<Utc>>,
        zone_id: Option<i32>,
    ) -> Result<Uuid> {
        info!("Creating order in MarketClearingService for user: {}", user_id);

        if energy_amount <= Decimal::ZERO {
            return Err(anyhow::anyhow!("Energy amount must be positive"));
        }

        let price_per_kwh_val = match order_type {
            OrderType::Limit => {
                let price = price_per_kwh.ok_or_else(|| {
                    anyhow::anyhow!("Price per kWh is required for Limit orders")
                })?;
                if price <= Decimal::ZERO {
                    return Err(anyhow::anyhow!("Price per kWh must be positive"));
                }
                price
            }
            OrderType::Market => Decimal::ZERO,
        };

        let order_id = Uuid::new_v4();
        let now = Utc::now();
        let expires_at = expiry_time.unwrap_or_else(|| now + Duration::days(1));

        // Get or create current epoch
        let epoch = self.get_or_create_epoch(now).await?;

        // 1. Start transaction
        let mut tx = self.db.begin().await?;

        // 2. Handle Escrow (Lock Funds/Energy)
        match side {
            OrderSide::Buy => {
                let total_escrow_amount = energy_amount * price_per_kwh_val;
                
                // Check balance
                let user = sqlx::query!("SELECT balance FROM users WHERE id = $1 FOR UPDATE", user_id)
                    .fetch_one(&mut *tx)
                    .await?;

                if user.balance.unwrap_or(Decimal::ZERO) < total_escrow_amount {
                    return Err(anyhow::anyhow!("Insufficient balance for escrow. Required: {}, Available: {}", total_escrow_amount, user.balance.unwrap_or(Decimal::ZERO)));
                }

                // Update user balance and locked_amount
                sqlx::query!(
                    "UPDATE users SET balance = balance - $1, locked_amount = locked_amount + $1 WHERE id = $2",
                    total_escrow_amount,
                    user_id
                )
                .execute(&mut *tx)
                .await?;

                // Create escrow record
                sqlx::query!(
                    r#"
                    INSERT INTO escrow_records (
                        user_id, order_id, amount, asset_type, escrow_type, status, description
                    ) VALUES ($1, $2, $3, 'currency', 'buy_lock', 'locked', $4)
                    "#,
                    user_id,
                    order_id,
                    total_escrow_amount,
                    format!("Buy order {} escrow", order_id)
                )
                .execute(&mut *tx)
                .await?;
            }
            OrderSide::Sell => {
                // Lock energy in DB
                sqlx::query!(
                    "UPDATE users SET locked_energy = locked_energy + $1 WHERE id = $2",
                    energy_amount,
                    user_id
                )
                .execute(&mut *tx)
                .await?;

                sqlx::query!(
                    r#"
                    INSERT INTO escrow_records (
                        user_id, order_id, amount, asset_type, escrow_type, status, description
                    ) VALUES ($1, $2, $3, 'energy', 'sell_lock', 'locked', $4)
                    "#,
                    user_id,
                    order_id,
                    energy_amount,
                    format!("Sell order {} energy lock", order_id)
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        // 3. Insert order into DB
        sqlx::query!(
            r#"
            INSERT INTO trading_orders (
                id, user_id, order_type, side, energy_amount, price_per_kwh,
                filled_amount, status, expires_at, created_at, epoch_id, zone_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
            order_id,
            user_id,
            order_type as OrderType,
            side as OrderSide,
            energy_amount,
            price_per_kwh_val,
            Decimal::ZERO,
            OrderStatus::Pending as OrderStatus,
            expires_at,
            now,
            epoch.id,
            zone_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        info!("Created order {} for user {} with assets escrowed", order_id, user_id);

        // Broadcast order created event
        self.websocket_service.broadcast_order_created(
            order_id.to_string(),
            energy_amount.to_f64().unwrap_or(0.0),
            price_per_kwh_val.to_f64().unwrap_or(0.0),
            match side {
                OrderSide::Buy => None,
                OrderSide::Sell => Some("solar".to_string()), // Simplified assumption
            },
            user_id.to_string(),
        ).await;

        // 2. Audit Log
        self.audit_logger.log_async(crate::services::AuditEvent::OrderCreated {
            user_id,
            order_id,
            order_type: format!("{:?}", side),
            amount: energy_amount.to_string(),
            price: price_per_kwh_val.to_string(),
        });

        // 3. On-Chain Order Creation
        self.execute_on_chain_order_creation(user_id, order_id, side, energy_amount, price_per_kwh_val).await?;

        Ok(order_id)
    }

    /// Update order status
    pub(super) async fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<()> {
        let status_str = match status {
            OrderStatus::Pending => "pending",
            OrderStatus::Active => "active",
            OrderStatus::PartiallyFilled => "partially_filled",
            OrderStatus::Filled => "filled",
            OrderStatus::Settled => "settled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Expired => "expired",
        };

        info!("Updating order {} status to: {}", order_id, status_str);

        let result =
            sqlx::query("UPDATE trading_orders SET status = $1::order_status WHERE id = $2")
                .bind(status_str)
                .bind(order_id)
                .execute(&self.db)
                .await?;

        info!(
            "Updated order {} status, rows affected: {}",
            order_id,
            result.rows_affected()
        );

        Ok(())
    }

    /// Update order filled amount
    pub(super) async fn update_order_filled_amount(&self, order_id: Uuid, amount: Decimal) -> Result<()> {
        sqlx::query!(
            "UPDATE trading_orders SET filled_amount = filled_amount + $1 WHERE id = $2",
            amount,
            order_id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Cancel an order
    pub async fn cancel_order(&self, order_id: Uuid, user_id: Uuid) -> Result<()> {
        // Check if order belongs to user and is still pending
        let order = sqlx::query!(
            "SELECT user_id, status as \"status: OrderStatus\" FROM trading_orders WHERE id = $1",
            order_id
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(order) = order {
            if order.user_id != user_id {
                return Err(
                    ApiError::Forbidden("Order does not belong to user".to_string()).into(),
                );
            }

            if !matches!(order.status, OrderStatus::Pending) {
                return Err(ApiError::BadRequest("Order cannot be cancelled".to_string()).into());
            }

            // Cancel order
            let status_str = "cancelled";
            sqlx::query(&format!(
                "UPDATE trading_orders SET status = '{}'::order_status, updated_at = NOW() WHERE id = $1", 
                status_str
            ))
            .bind(order_id)
            .execute(&self.db)
            .await?;

            info!("Order {} cancelled by user {}", order_id, user_id);
        } else {
            return Err(ApiError::NotFound("Order not found".to_string()).into());
        }

        Ok(())
    }

    /// Get trading history for a user
    pub async fn get_trading_history(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Settlement>> {
        let settlements = sqlx::query_as!(
            Settlement,
            r#"
            SELECT 
                id as "id!", epoch_id as "epoch_id!", buyer_id as "buyer_id!", seller_id as "seller_id!", 
                energy_amount as "energy_amount!", price_per_kwh as "price_per_kwh!", 
                total_amount as "total_amount!", fee_amount as "fee_amount!", 
                wheeling_charge as "wheeling_charge!", loss_factor as "loss_factor!", 
                loss_cost as "loss_cost!", effective_energy as "effective_energy!", 
                buyer_zone_id as "buyer_zone_id", seller_zone_id as "seller_zone_id", 
                net_amount as "net_amount!", status as "status!"
            FROM settlements 
            WHERE buyer_id = $1 OR seller_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            limit,
            offset
        )
        .fetch_all(&self.db)
        .await?;

        Ok(settlements)
    }
}
