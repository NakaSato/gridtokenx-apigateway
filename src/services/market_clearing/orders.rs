use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::Row;
use uuid::Uuid;
use tracing::{info, error};

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
        // energy_amount in the query is the remaining amount (original - filled)
        let buy_orders: Vec<OrderBookEntry> = sqlx::query_as!(
            OrderBookEntry,
            r#"
            SELECT 
                id as order_id, user_id, side as "side!: OrderSide", 
                (energy_amount - COALESCE(filled_amount, 0)) as "energy_amount!",
                energy_amount as "original_amount!",
                price_per_kwh as "price_per_kwh!", created_at as "created_at!", zone_id
            FROM trading_orders 
            WHERE status IN ('pending', 'partially_filled') AND side = 'buy' AND epoch_id = $1 AND price_per_kwh IS NOT NULL
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
                (energy_amount - COALESCE(filled_amount, 0)) as "energy_amount!",
                energy_amount as "original_amount!",
                price_per_kwh as "price_per_kwh!", created_at as "created_at!", zone_id
            FROM trading_orders 
            WHERE status IN ('pending', 'partially_filled') AND side = 'sell' AND epoch_id = $1 AND price_per_kwh IS NOT NULL
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
        meter_id: Option<Uuid>,
        session_token: Option<&str>,
    ) -> Result<Uuid> {
        info!("Creating order in MarketClearingService for user: {}, meter: {:?}", user_id, meter_id);

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

        // 2. Insert order into DB (Must process first to satisfy FK for escrow_records)
        sqlx::query!(
            r#"
            INSERT INTO trading_orders (
                id, user_id, order_type, side, energy_amount, price_per_kwh,
                filled_amount, status, expires_at, created_at, epoch_id, zone_id, meter_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
            zone_id,
            meter_id
        )
        .execute(&mut *tx)
        .await?;

        // 3. Fetch user (for balance/wallet check)
        // Must happen inside transaction for lock stability if we are checking DB balance
        let user = sqlx::query!(
            "SELECT balance, wallet_address FROM users WHERE id = $1 FOR UPDATE", 
            user_id
        )
        .fetch_one(&mut *tx)
        .await?;

        // 4. Handle Escrow (Lock Funds/Energy)
        match side {
            OrderSide::Buy => {
                let total_escrow_amount = energy_amount * price_per_kwh_val;

                // 2. On-Chain Balance Check (Optional/Configurable)
                let use_onchain_balance = self.config.tokenization.use_onchain_balance_for_escrow;
                
                if use_onchain_balance {
                    use std::str::FromStr;
                    use solana_sdk::pubkey::Pubkey;

                    // Get user wallet from DB
                    let user_wallet_str = match &user.wallet_address {
                         Some(w) => w,
                         None => return Err(anyhow::anyhow!("User wallet address required for on-chain check"))
                    };
                    
                    let user_wallet = Pubkey::from_str(user_wallet_str)
                        .map_err(|e| anyhow::anyhow!("Invalid user wallet address: {}", e))?;
                        
                    let currency_mint = Pubkey::from_str(&self.config.currency_token_mint)
                        .map_err(|e| anyhow::anyhow!("Invalid currency mint config: {}", e))?;

                    // Convert required amount to token units (e.g. 6 decimals for USDC)
                    let decimals = self.config.currency_decimals;
                    let required_tokens = (total_escrow_amount * Decimal::from(10u64.pow(decimals as u32)))
                        .to_u64()
                        .ok_or_else(|| anyhow::anyhow!("Amount too large"))?;

                    let balance = self.blockchain_service.get_token_balance(&user_wallet, &currency_mint).await?;
                    
                    info!("On-chain balance check for user {}: has {} tokens, needs {}", user_id, balance, required_tokens);

                    if balance < required_tokens {
                         return Err(anyhow::anyhow!("Insufficient on-chain balance. Required: {}, Available: {}", required_tokens, balance));
                    }
                }

                // 3. Database Balance Check (Always perform for internal consistency)
                if user.balance.unwrap_or(Decimal::ZERO) < total_escrow_amount {
                    return Err(anyhow::anyhow!("Insufficient DB balance for escrow. Required: {}, Available: {}", total_escrow_amount, user.balance.unwrap_or(Decimal::ZERO)));
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
                // 1. On-Chain Energy Balance Check (Optional/Configurable)
                let use_onchain_balance = self.config.tokenization.use_onchain_balance_for_escrow;

                if use_onchain_balance {
                    use std::str::FromStr;
                    use solana_sdk::pubkey::Pubkey;

                    // Get user wallet from DB (user variable is available now)
                    let user_wallet_str = match &user.wallet_address {
                         Some(w) => w,
                         None => return Err(anyhow::anyhow!("User wallet address required for on-chain check"))
                    };
                    
                    let user_wallet = Pubkey::from_str(user_wallet_str)
                        .map_err(|e| anyhow::anyhow!("Invalid user wallet address: {}", e))?;

                    let energy_mint = Pubkey::from_str(&self.config.energy_token_mint)
                        .map_err(|e| anyhow::anyhow!("Invalid energy mint config: {}", e))?;
                    
                    // Energy tokens usually have 9 decimals (same as SOL)
                    // TODO: Move energy decimals to config if variable
                    let decimals = 9; 
                    let required_tokens = (energy_amount * Decimal::from(10u64.pow(decimals)))
                        .to_u64()
                        .ok_or_else(|| anyhow::anyhow!("Energy amount too large"))?;

                    let balance = self.blockchain_service.get_token_balance(&user_wallet, &energy_mint).await?;
                    
                    info!("On-chain energy check for user {}: has {} tokens, needs {}", user_id, balance, required_tokens);

                    if balance < required_tokens {
                        return Err(anyhow::anyhow!("Insufficient on-chain energy balance. Required: {}, Available: {}", required_tokens, balance));
                    }
                }

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



        // Fetch meter type for broadcasting if available (before commit)
        let mut energy_source_type: Option<String> = None;
        if let Some(mid) = meter_id {
             if let Ok(Some(rec)) = sqlx::query!(
                "SELECT meter_type FROM meter_registry WHERE id = $1",
                mid
            )
            .fetch_optional(&mut *tx)
            .await 
            {
                energy_source_type = rec.meter_type;
            }
        }

        tx.commit().await?;

        info!("Created order {} for user {} with assets escrowed", order_id, user_id);

        // Broadcast order created event
        self.websocket_service.broadcast_order_created(
            order_id.to_string(),
            energy_amount.to_f64().unwrap_or(0.0),
            price_per_kwh_val.to_f64().unwrap_or(0.0),
            match side {
                OrderSide::Buy => None,
                OrderSide::Sell => energy_source_type.or(Some("solar".to_string())),
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
        self.execute_on_chain_order_creation(user_id, order_id, side, energy_amount, price_per_kwh_val, session_token).await?;

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

    /// Cancel an order and refund the unfilled escrow amount
    pub async fn cancel_order(&self, order_id: Uuid, user_id: Uuid) -> Result<()> {
        use crate::handlers::websocket::broadcaster::broadcast_p2p_order_update;
        
        // Get full order details including filled amount
        let order = sqlx::query!(
            r#"
            SELECT user_id, side as "side!: OrderSide", status as "status: OrderStatus", 
                   energy_amount, filled_amount, price_per_kwh as "price_per_kwh"
            FROM trading_orders 
            WHERE id = $1
            "#,
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

            // Allow cancellation for pending or partially_filled orders
            if !matches!(order.status, OrderStatus::Pending | OrderStatus::PartiallyFilled) {
                return Err(ApiError::BadRequest(format!(
                    "Order cannot be cancelled (status: {:?})", order.status
                )).into());
            }

            // Calculate unfilled amount that needs to be refunded
            let filled = order.filled_amount.unwrap_or(Decimal::ZERO);
            let original = order.energy_amount;
            let unfilled = original - filled;

            if unfilled <= Decimal::ZERO {
                return Err(ApiError::BadRequest(
                    "Order is fully filled and cannot be cancelled".to_string()
                ).into());
            }

            // price_per_kwh is Decimal (not null in trading_orders)
            let price = order.price_per_kwh;

            // Start transaction for atomicity
            let mut tx = self.db.begin().await?;

            // Refund based on order side
            match order.side {
                OrderSide::Buy => {
                    // Return locked funds for unfilled portion
                    let refund_amount = unfilled * price;
                    sqlx::query!(
                        "UPDATE users SET balance = balance + $1, locked_amount = locked_amount - $1 WHERE id = $2",
                        refund_amount,
                        user_id
                    )
                    .execute(&mut *tx)
                    .await?;

                    info!(
                        "Refunded {} to user {} for cancelled buy order {} (unfilled: {} kWh @ {})",
                        refund_amount, user_id, order_id, unfilled, price
                    );
                }
                OrderSide::Sell => {
                    // Return locked energy for unfilled portion
                    sqlx::query!(
                        "UPDATE users SET locked_energy = locked_energy - $1 WHERE id = $2",
                        unfilled,
                        user_id
                    )
                    .execute(&mut *tx)
                    .await?;

                    info!(
                        "Unlocked {} kWh energy for user {} from cancelled sell order {}",
                        unfilled, user_id, order_id
                    );
                }
            }

            // Update escrow record status
            sqlx::query!(
                "UPDATE escrow_records SET status = 'released', description = $1, updated_at = NOW() WHERE order_id = $2 AND status = 'locked'",
                format!("Order cancelled - refunded unfilled portion: {}", unfilled),
                order_id
            )
            .execute(&mut *tx)
            .await?;

            // Update order status to cancelled
            sqlx::query(
                "UPDATE trading_orders SET status = 'cancelled'::order_status, updated_at = NOW() WHERE id = $1"
            )
            .bind(order_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            // Broadcast cancellation via WebSocket
            let _ = broadcast_p2p_order_update(
                order_id,
                user_id,
                match order.side {
                    OrderSide::Buy => "buy".to_string(),
                    OrderSide::Sell => "sell".to_string(),
                },
                "cancelled".to_string(),
                original.to_string(),
                filled.to_string(),
                "0".to_string(), // remaining is 0 after cancel
                price.to_string(),
            ).await;

            info!("Order {} cancelled by user {} (filled: {}, refunded: {})", 
                order_id, user_id, filled, unfilled);

            // Execute On-Chain Refund
            // Buy Order -> Refund Currency (unfilled * price)
            // Sell Order -> Refund Energy (unfilled)
            let (asset_type, refund_amount) = match order.side {
                OrderSide::Buy => ("currency", unfilled * price),
                OrderSide::Sell => ("energy", unfilled),
            };

            if refund_amount > Decimal::ZERO {
                match self.execute_escrow_refund(user_id, refund_amount, asset_type).await {
                    Ok(sig) => {
                        info!("On-chain escrow refund executed for order {}: {}", order_id, sig);
                    }
                    Err(e) => {
                         error!("Failed to execute on-chain refund for order {}: {}. Queueing for retry.", order_id, e);
                         
                         // Queue for manual retry
                         let payload = serde_json::json!({
                             "type": "EscrowRefund", 
                             "data": {
                                 "user_id": user_id,
                                 "amount": refund_amount,
                                 "asset_type": asset_type,
                                 "order_id": order_id
                             }
                         });
                         
                         let _ = self.queue_blockchain_task("escrow_refund", payload).await.map_err(|qe| {
                             error!("CRITICAL: Failed to queue blockchain task: {}", qe);
                             qe
                         });
                    }
                }
            }

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
        let settlements = sqlx::query(
            r#"
            SELECT 
                id, epoch_id, buyer_id, seller_id, 
                energy_amount, price_per_kwh, 
                total_amount, fee_amount, 
                wheeling_charge, loss_factor, 
                loss_cost, effective_energy, 
                buyer_zone_id, seller_zone_id, 
                net_amount, status,
                buyer_session_token, seller_session_token
            FROM settlements 
            WHERE buyer_id = $1 OR seller_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await?;

        let result = settlements.into_iter().map(|row| Settlement {
            id: row.get("id"),
            epoch_id: row.get("epoch_id"),
            buyer_id: row.get("buyer_id"),
            seller_id: row.get("seller_id"),
            energy_amount: row.get("energy_amount"),
            price_per_kwh: row.get("price_per_kwh"),
            total_amount: row.get("total_amount"),
            fee_amount: row.get("fee_amount"),
            wheeling_charge: row.get("wheeling_charge"),
            loss_factor: row.get("loss_factor"),
            loss_cost: row.get("loss_cost"),
            effective_energy: row.get("effective_energy"),
            buyer_zone_id: row.get("buyer_zone_id"),
            seller_zone_id: row.get("seller_zone_id"),
            net_amount: row.get("net_amount"),
            status: row.get("status"),
            buyer_session_token: row.get("buyer_session_token"),
            seller_session_token: row.get("seller_session_token"),
        }).collect();

        Ok(result)
    }

    /// Queue a blockchain task for retry
    async fn queue_blockchain_task(&self, task_type: &str, payload: serde_json::Value) -> Result<Uuid> {
        let id = sqlx::query!(
            r#"
            INSERT INTO blockchain_tasks (task_type, payload, status, next_retry_at)
            VALUES ($1::blockchain_task_type, $2, 'pending', NOW())
            RETURNING id
            "#,
            task_type as _,
            payload
        )
        .fetch_one(&self.db)
        .await?
        .id;
        
        info!("Queued blockchain task {} (type: {})", id, task_type);
        Ok(id)
    }
}
