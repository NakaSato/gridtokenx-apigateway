use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use sqlx::PgPool;
use std::str::FromStr;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::schema::types::{EpochStatus, OrderSide, OrderStatus};
use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct MarketEpoch {
    pub id: Uuid,
    pub epoch_number: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: EpochStatus,
    pub clearing_price: Option<BigDecimal>,
    pub total_volume: Option<BigDecimal>,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct OrderMatch {
    pub id: Uuid,
    pub epoch_id: Uuid,
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub matched_amount: BigDecimal,
    pub match_price: BigDecimal,
    pub match_time: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct Settlement {
    pub id: Uuid,
    pub epoch_id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    pub energy_amount: BigDecimal,
    pub price_per_kwh: BigDecimal,
    pub total_amount: BigDecimal,
    pub fee_amount: BigDecimal,
    pub net_amount: BigDecimal,
    pub status: String,
}

#[derive(Debug)]
pub struct OrderBookEntry {
    pub order_id: Uuid,
    pub user_id: Uuid,
    pub side: Option<OrderSide>,
    pub energy_amount: BigDecimal,
    pub price_per_kwh: BigDecimal,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct MarketClearingService {
    db: PgPool,
}

impl MarketClearingService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Get current market epoch (15-minute intervals)
    pub async fn get_current_epoch(&self) -> Result<Option<MarketEpoch>> {
        let epoch = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT 
                id, epoch_number, start_time, end_time, status as "status: EpochStatus",
                clearing_price, 
                total_volume as "total_volume?", 
                total_orders as "total_orders?", 
                matched_orders as "matched_orders?"
            FROM market_epochs 
            WHERE start_time <= NOW() AND end_time > NOW()
            ORDER BY start_time DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(epoch)
    }

    /// Create or get market epoch for a specific timestamp
    pub async fn get_or_create_epoch(&self, timestamp: DateTime<Utc>) -> Result<MarketEpoch> {
        // Calculate epoch number: YYYYMMDDHHMM (15-minute intervals)
        let epoch_number = (timestamp.year() as i64) * 100_000_000
            + (timestamp.month() as i64) * 1_000_000
            + (timestamp.day() as i64) * 10_000
            + (timestamp.hour() as i64) * 100
            + ((timestamp.minute() / 15) * 15) as i64;

        // Calculate epoch start and end times
        let epoch_start = timestamp
            .with_minute((timestamp.minute() / 15) * 15)
            .and_then(|dt| dt.with_second(0))
            .and_then(|dt| dt.with_nanosecond(0))
            .unwrap_or(timestamp);

        let epoch_end = epoch_start + Duration::minutes(15);

        // Try to get existing epoch
        if let Some(mut existing) = self.get_epoch_by_number(epoch_number).await? {
            // Update epoch status based on current time
            let now = Utc::now();
            let new_status = if now >= epoch_start && now < epoch_end {
                EpochStatus::Active
            } else if now >= epoch_end {
                EpochStatus::Cleared
            } else {
                existing.status.clone()
            };

            if new_status != existing.status {
                let status_str = match new_status {
                    EpochStatus::Pending => "pending",
                    EpochStatus::Active => "active",
                    EpochStatus::Cleared => "cleared",
                    EpochStatus::Settled => "settled",
                };

                sqlx::query(&format!("UPDATE market_epochs SET status = '{}'::epoch_status, updated_at = NOW() WHERE id = $1", status_str))
                    .bind(existing.id)
                    .execute(&self.db)
                    .await?;

                // Update the existing epoch status for return
                existing.status = new_status;
            }

            return Ok(existing);
        }

        // Create new epoch
        let epoch_id = Uuid::new_v4();
        let epoch = MarketEpoch {
            id: epoch_id,
            epoch_number,
            start_time: epoch_start,
            end_time: epoch_end,
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: None,
            total_orders: None,
            matched_orders: None,
        };

        let status_str = "pending";
        sqlx::query(&format!(
            r#"
            INSERT INTO market_epochs (
                id, epoch_number, start_time, end_time, status
            ) VALUES ($1, $2, $3, $4, '{}'::epoch_status)
            "#,
            status_str
        ))
        .bind(epoch.id)
        .bind(epoch.epoch_number)
        .bind(epoch.start_time)
        .bind(epoch.end_time)
        .execute(&self.db)
        .await?;

        info!(
            "Created new market epoch: {} ({})",
            epoch.id, epoch.epoch_number
        );
        Ok(epoch)
    }

    /// Get epoch by epoch number
    pub async fn get_epoch_by_number(&self, epoch_number: i64) -> Result<Option<MarketEpoch>> {
        let epoch = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT 
                id, epoch_number, start_time, end_time, status as "status: EpochStatus",
                clearing_price, total_volume, total_orders, matched_orders
            FROM market_epochs 
            WHERE epoch_number = $1
            "#,
            epoch_number
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(epoch)
    }

    /// Get current order book for an epoch
    pub async fn get_order_book(
        &self,
        epoch_id: Uuid,
    ) -> Result<(Vec<OrderBookEntry>, Vec<OrderBookEntry>)> {
        info!("Getting order book for epoch: {}", epoch_id);

        // Get pending buy orders (sorted by price descending, then time ascending)
        let buy_orders = sqlx::query_as!(
            OrderBookEntry,
            r#"
            SELECT 
                id as order_id, user_id, side as "side: OrderSide", 
                energy_amount, price_per_kwh, created_at
            FROM trading_orders 
            WHERE status = 'pending' AND side = 'buy' AND epoch_id = $1
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
        let sell_orders = sqlx::query_as!(
            OrderBookEntry,
            r#"
            SELECT 
                id as order_id, user_id, side as "side: OrderSide", 
                energy_amount, price_per_kwh, created_at
            FROM trading_orders 
            WHERE status = 'pending' AND side = 'sell' AND epoch_id = $1
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

    /// Run order matching algorithm for an epoch
    pub async fn run_order_matching(&self, epoch_id: Uuid) -> Result<Vec<OrderMatch>> {
        info!("Starting order matching for epoch: {}", epoch_id);

        // Get current order book
        let (mut buy_orders, mut sell_orders) = self.get_order_book(epoch_id).await?;

        if buy_orders.is_empty() || sell_orders.is_empty() {
            info!("No orders to match in epoch: {}", epoch_id);
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let mut total_volume = BigDecimal::from_str("0").unwrap();
        let mut total_match_count = 0;

        // Order matching algorithm: price-time priority
        while let Some(buy_order) = buy_orders.first_mut() {
            if let Some(sell_order) = sell_orders.first_mut() {
                // Check if orders can be matched
                if buy_order.price_per_kwh >= sell_order.price_per_kwh {
                    // Determine match price (use sell order price for simplicity)
                    let match_price = sell_order.price_per_kwh.clone(); // Market clearing price

                    // Calculate match amount (minimum of remaining amounts)
                    let match_amount = buy_order
                        .energy_amount
                        .clone()
                        .min(sell_order.energy_amount.clone());

                    if match_amount > BigDecimal::from_str("0").unwrap() {
                        let match_amount_clone = match_amount.clone();
                        let match_price_clone = match_price.clone();

                        // Create order match
                        let order_match = OrderMatch {
                            id: Uuid::new_v4(),
                            epoch_id,
                            buy_order_id: buy_order.order_id,
                            sell_order_id: sell_order.order_id,
                            matched_amount: match_amount_clone.clone(),
                            match_price: match_price_clone.clone(),
                            match_time: Utc::now(),
                            status: "pending".to_string(),
                        };

                        // Save match to database
                        self.save_order_match(&order_match).await?;
                        matches.push(order_match);

                        // Update order amounts
                        buy_order.energy_amount -= match_amount_clone.clone();
                        sell_order.energy_amount -= match_amount_clone.clone();

                        // Update totals
                        total_volume += match_amount_clone.clone();
                        total_match_count += 1;

                        info!(
                            "Matched orders: Buy {} vs Sell {} at {} for {} kWh",
                            buy_order.order_id,
                            sell_order.order_id,
                            match_price_clone,
                            match_amount_clone
                        );

                        // Remove fully filled orders
                        info!(
                            "Buy order {} remaining amount: {}",
                            buy_order.order_id, buy_order.energy_amount
                        );
                        if buy_order.energy_amount <= BigDecimal::from_str("0").unwrap() {
                            info!(
                                "Buy order {} is fully filled, updating status",
                                buy_order.order_id
                            );
                            self.update_order_status(buy_order.order_id, OrderStatus::Filled)
                                .await?;
                            buy_orders.remove(0);
                        } else {
                            info!(
                                "Buy order {} is partially filled, updating amount",
                                buy_order.order_id
                            );
                            self.update_order_filled_amount(
                                buy_order.order_id,
                                match_amount_clone.clone(),
                            )
                            .await?;
                        }

                        info!(
                            "Sell order {} remaining amount: {}",
                            sell_order.order_id, sell_order.energy_amount
                        );
                        if sell_order.energy_amount <= BigDecimal::from_str("0").unwrap() {
                            info!(
                                "Sell order {} is fully filled, updating status",
                                sell_order.order_id
                            );
                            self.update_order_status(sell_order.order_id, OrderStatus::Filled)
                                .await?;
                            sell_orders.remove(0);
                        } else {
                            info!(
                                "Sell order {} is partially filled, updating amount",
                                sell_order.order_id
                            );
                            self.update_order_filled_amount(
                                sell_order.order_id,
                                match_amount_clone.clone(),
                            )
                            .await?;
                        }
                    }
                } else {
                    // No more matches possible (best buy price < best sell price)
                    break;
                }
            } else {
                break;
            }
        }

        // Update epoch statistics
        self.update_epoch_statistics(epoch_id, total_volume.clone(), total_match_count)
            .await?;

        // Calculate and set clearing price (average of match prices)
        if !matches.is_empty() {
            let total_match_value: BigDecimal = matches
                .iter()
                .map(|m| m.matched_amount.clone() * m.match_price.clone())
                .fold(BigDecimal::from_str("0").unwrap(), |acc, val| acc + val);
            let clearing_price = total_match_value / total_volume.clone();

            sqlx::query!(
                "UPDATE market_epochs SET clearing_price = $1 WHERE id = $2",
                clearing_price,
                epoch_id
            )
            .execute(&self.db)
            .await?;
        }

        // Create settlements for all matches
        for order_match in &matches {
            if let Err(e) = self.create_settlement(order_match).await {
                error!(
                    "Failed to create settlement for match {}: {}",
                    order_match.id, e
                );
            }
        }

        info!(
            "Order matching completed for epoch: {} - {} matches, {} kWh",
            epoch_id,
            matches.len(),
            total_volume
        );

        Ok(matches)
    }

    /// Save order match to database
    async fn save_order_match(&self, order_match: &OrderMatch) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO order_matches (
                id, epoch_id, buy_order_id, sell_order_id, 
                matched_amount, match_price, match_time, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            order_match.id,
            order_match.epoch_id,
            order_match.buy_order_id,
            order_match.sell_order_id,
            order_match.matched_amount,
            order_match.match_price,
            order_match.match_time,
            order_match.status
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Update order status
    async fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<()> {
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

        let result = sqlx::query(
            "UPDATE trading_orders SET status = $1::order_status, filled_at = NOW() WHERE id = $2",
        )
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
    async fn update_order_filled_amount(&self, order_id: Uuid, amount: BigDecimal) -> Result<()> {
        sqlx::query!(
            "UPDATE trading_orders SET filled_amount = filled_amount + $1 WHERE id = $2",
            amount,
            order_id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Update epoch statistics
    async fn update_epoch_statistics(
        &self,
        epoch_id: Uuid,
        total_volume: BigDecimal,
        matched_orders: i64,
    ) -> Result<()> {
        // Get total orders count for this epoch
        let total_orders = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM trading_orders WHERE epoch_id = $1 AND status IN ('pending', 'filled')",
            epoch_id
        )
        .fetch_one(&self.db)
        .await?
        .unwrap_or(0);

        let status_str = "cleared";
        sqlx::query(&format!(
            r#"
            UPDATE market_epochs 
            SET total_volume = $1, matched_orders = $2, total_orders = $3, status = '{}'::epoch_status
            WHERE id = $4
            "#, status_str
        ))
        .bind(total_volume)
        .bind(matched_orders)
        .bind(total_orders)
        .bind(epoch_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Create settlement for an order match
    async fn create_settlement(&self, order_match: &OrderMatch) -> Result<Settlement> {
        // Get buyer and seller information from orders
        let buy_order = sqlx::query!(
            "SELECT user_id FROM trading_orders WHERE id = $1",
            order_match.buy_order_id
        )
        .fetch_one(&self.db)
        .await?;

        let sell_order = sqlx::query!(
            "SELECT user_id FROM trading_orders WHERE id = $1",
            order_match.sell_order_id
        )
        .fetch_one(&self.db)
        .await?;

        // Calculate settlement amounts
        let total_amount = order_match.matched_amount.clone() * order_match.match_price.clone();
        let fee_rate = BigDecimal::from_str("0.01").unwrap(); // 1% fee
        let fee_amount = total_amount.clone() * fee_rate.clone();
        let net_amount = total_amount.clone() - fee_amount.clone();

        let settlement = Settlement {
            id: Uuid::new_v4(),
            epoch_id: order_match.epoch_id,
            buyer_id: buy_order.user_id,
            seller_id: sell_order.user_id,
            energy_amount: order_match.matched_amount.clone(),
            price_per_kwh: order_match.match_price.clone(),
            total_amount: total_amount.clone(),
            fee_amount: fee_amount.clone(),
            net_amount: net_amount.clone(),
            status: "pending".to_string(),
        };

        // Save settlement
        sqlx::query!(
            r#"
            INSERT INTO settlements (
                id, epoch_id, buyer_id, seller_id, energy_amount, 
                price_per_kwh, total_amount, fee_amount, net_amount, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            settlement.id,
            settlement.epoch_id,
            settlement.buyer_id,
            settlement.seller_id,
            settlement.energy_amount,
            settlement.price_per_kwh,
            settlement.total_amount,
            settlement.fee_amount,
            settlement.net_amount,
            settlement.status
        )
        .execute(&self.db)
        .await?;

        // Update order match with settlement ID
        sqlx::query!(
            "UPDATE order_matches SET settlement_id = $1 WHERE id = $2",
            settlement.id,
            order_match.id
        )
        .execute(&self.db)
        .await?;

        Ok(settlement)
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
                id, epoch_id, buyer_id, seller_id, energy_amount,
                price_per_kwh, total_amount, fee_amount, net_amount, status
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

    /// Get market statistics for recent epochs
    pub async fn get_market_statistics(&self, epochs: i64) -> Result<Vec<MarketEpoch>> {
        let stats = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT 
                id, epoch_number, start_time, end_time, status as "status: EpochStatus",
                clearing_price, total_volume, total_orders, matched_orders
            FROM market_epochs 
            WHERE status IN ('cleared', 'settled')
            ORDER BY epoch_number DESC
            LIMIT $1
            "#,
            epochs
        )
        .fetch_all(&self.db)
        .await?;

        Ok(stats)
    }
}
