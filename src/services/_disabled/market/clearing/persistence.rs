use crate::error::ApiError;
// use crate::models::trading::TradingOrder; // Unused
use crate::services::market::order_book::OrderBook;
use crate::services::market::types::{BookOrder, OrderSide, TradeMatch};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use rust_decimal::Decimal;
use serde_json;
use sqlx::PgPool;
use std::str::FromStr;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct ClearingPersistence {
    db: PgPool,
    redis: redis::Client,
}

impl ClearingPersistence {
    pub fn new(db: PgPool, redis: redis::Client) -> Self {
        Self { db, redis }
    }

    /// Save order book snapshot to Redis
    pub async fn save_order_book_snapshot(&self, book: &OrderBook) -> Result<(), ApiError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {}", e)))?;

        // Clear existing keys first to avoid stale data
        let _: () = conn
            .del("order_book:buy")
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL failed: {}", e)))?;
        let _: () = conn
            .del("order_book:sell")
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL failed: {}", e)))?;

        // Save each buy order to Redis sorted set
        for (_price_key, level) in book.buy_levels.iter() {
            for order in &level.orders {
                let order_json = serde_json::to_string(order)
                    .map_err(|e| ApiError::Internal(format!("Serialization failed: {}", e)))?;

                // Store in sorted set with price as score
                let _: () = conn
                    .zadd(
                        "order_book:buy",
                        order_json.clone(),
                        order.price.to_string().parse::<f64>().unwrap_or(0.0),
                    )
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis ZADD failed: {}", e)))?;

                // Store order details in hash
                let _: () = conn
                    .hset(format!("order:{}", order.id), "data", order_json)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis HSET failed: {}", e)))?;

                // Set expiration (24 hours)
                let _: () = conn
                    .expire(format!("order:{}", order.id), 86400)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis EXPIRE failed: {}", e)))?;
            }
        }

        // Save each sell order to Redis sorted set
        for (_price_key, level) in book.sell_levels.iter() {
            for order in &level.orders {
                let order_json = serde_json::to_string(order)
                    .map_err(|e| ApiError::Internal(format!("Serialization failed: {}", e)))?;

                let _: () = conn
                    .zadd(
                        "order_book:sell",
                        order_json.clone(),
                        order.price.to_string().parse::<f64>().unwrap_or(0.0),
                    )
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis ZADD failed: {}", e)))?;

                let _: () = conn
                    .hset(format!("order:{}", order.id), "data", order_json)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis HSET failed: {}", e)))?;

                let _: () = conn
                    .expire(format!("order:{}", order.id), 86400)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Redis EXPIRE failed: {}", e)))?;
            }
        }

        // Save metadata
        let metadata = serde_json::json!({
            "best_bid": book.best_bid(),
            "best_ask": book.best_ask(),
            "mid_price": book.mid_price(),
            "spread": book.spread(),
            "updated_at": Utc::now(),
        });

        let _: () = conn
            .set("order_book:metadata", metadata.to_string())
            .await
            .map_err(|e| ApiError::Internal(format!("Redis SET failed: {}", e)))?;

        let _: () = conn
            .expire("order_book:metadata", 86400)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis EXPIRE failed: {}", e)))?;

        debug!("📸 Order book snapshot saved to Redis");
        Ok(())
    }

    /// Restore order book from Redis
    pub async fn restore_order_book_from_redis(
        &self,
        book: &mut OrderBook,
    ) -> Result<usize, ApiError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {}", e)))?;

        book.clear();
        let mut restored_count = 0;

        // Restore buy orders
        let buy_orders: Vec<(String, f64)> = conn
            .zrange_withscores("order_book:buy", 0, -1)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis ZRANGE failed: {}", e)))?;

        for (order_json, _score) in buy_orders {
            match serde_json::from_str::<BookOrder>(&order_json) {
                Ok(order) => {
                    if !order.is_expired() {
                        book.add_order(order);
                        restored_count += 1;
                    }
                }
                Err(e) => {
                    warn!("Failed to deserialize buy order: {}", e);
                }
            }
        }

        // Restore sell orders
        let sell_orders: Vec<(String, f64)> = conn
            .zrange_withscores("order_book:sell", 0, -1)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis ZRANGE failed: {}", e)))?;

        for (order_json, _score) in sell_orders {
            match serde_json::from_str::<BookOrder>(&order_json) {
                Ok(order) => {
                    if !order.is_expired() {
                        book.add_order(order);
                        restored_count += 1;
                    }
                }
                Err(e) => {
                    warn!("Failed to deserialize sell order: {}", e);
                }
            }
        }

        info!("🔄 Restored {} orders from Redis cache", restored_count);
        Ok(restored_count)
    }

    /// Clear Redis order book cache
    #[allow(dead_code)]
    pub async fn clear_redis_cache(&self) -> Result<(), ApiError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {}", e)))?;

        let _: () = conn
            .del("order_book:buy")
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL failed: {}", e)))?;
        let _: () = conn
            .del("order_book:sell")
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL failed: {}", e)))?;
        let _: () = conn
            .del("order_book:metadata")
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL failed: {}", e)))?;

        debug!("🗑️  Cleared Redis order book cache");
        Ok(())
    }

    /// Load active orders from database into order book
    pub async fn load_order_book(&self, book: &mut OrderBook) -> Result<usize, ApiError> {
        // Try to restore from Redis cache first
        match self.restore_order_book_from_redis(book).await {
            Ok(count) if count > 0 => {
                info!("✅ Restored {} orders from Redis cache", count);
                return Ok(count);
            }
            Ok(_) => {
                info!("📭 Redis cache empty, loading from database");
            }
            Err(e) => {
                warn!(
                    "⚠️  Failed to restore from Redis: {}, falling back to database",
                    e
                );
            }
        }

        // Load from database
        let orders = sqlx::query_as::<
            _,
            (
                Uuid,                  // id
                Uuid,                  // user_id
                Uuid,                  // epoch_id
                String,                // side
                String,                // energy_amount
                String,                // price_per_kwh
                String,                // filled_amount
                DateTime<Utc>,         // created_at
                DateTime<Utc>,         // expires_at
                String,                // status
                Option<DateTime<Utc>>, // filled_at
            ),
        >(
            r#"
            SELECT
                id, user_id, epoch_id, side::text, energy_amount::text,
                price_per_kwh::text, filled_amount::text,
                created_at, expires_at, status::text, filled_at
            FROM trading_orders
            WHERE status = 'pending'
                AND expires_at > NOW()
            ORDER BY created_at ASC
        "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        book.clear();
        let mut loaded_count = 0;

        for (
            id,
            user_id,
            epoch_id,
            side_str,
            energy_str,
            price_str,
            filled_str,
            created_at,
            expires_at,
            _status,
            _filled_at,
        ) in orders
        {
            let side = match side_str.as_str() {
                "buy" | "Buy" => OrderSide::Buy,
                "sell" | "Sell" => OrderSide::Sell,
                _ => continue,
            };

            // Fix: Decimal::from_str is more robust than from_str_exact for some cases, but sticking to exact if DB is reliable
            let energy_amount = Decimal::from_str(&energy_str)
                .map_err(|_| ApiError::Internal("Invalid energy amount".into()))?;
            let price = Decimal::from_str(&price_str)
                .map_err(|_| ApiError::Internal("Invalid price".into()))?;
            let filled_amount = Decimal::from_str(&filled_str)
                .map_err(|_| ApiError::Internal("Invalid filled amount".into()))?;

            let order = BookOrder {
                id,
                user_id,
                epoch_id: Some(epoch_id),
                side,
                energy_amount,
                price,
                filled_amount,
                created_at,
                expires_at,
            };

            book.add_order(order);
            loaded_count += 1;
        }

        info!("📚 Loaded {} active orders from database", loaded_count);

        // Save to Redis for future quick restores
        if loaded_count > 0 {
            if let Err(e) = self.save_order_book_snapshot(book).await {
                warn!("Failed to save order book to Redis: {}", e);
            }
        }

        Ok(loaded_count)
    }

    /// Mark confirmed expired orders as expired in DB
    pub async fn mark_orders_as_expired(&self, expired_ids: &[Uuid]) {
        for order_id in expired_ids {
            let _ = sqlx::query("UPDATE trading_orders SET status = 'expired' WHERE id = $1")
                .bind(order_id)
                .execute(&self.db)
                .await;
        }
    }

    /// Persist matched trades to database with atomic order updates
    pub async fn persist_matches(
        &self,
        matches: Vec<TradeMatch>,
        book_snapshot_for_cache: Option<&OrderBook>,
    ) -> Result<usize, ApiError> {
        if matches.is_empty() {
            return Ok(0);
        }

        let mut persisted = 0;

        for trade in matches {
            // Start transaction for atomic updates
            let mut tx = self.db.begin().await.map_err(ApiError::Database)?;

            // Create trade record
            let trade_id = Uuid::new_v4();

            // Get epoch_id from buy order
            let epoch_id = sqlx::query_scalar!(
                "SELECT epoch_id FROM trading_orders WHERE id = $1",
                trade.buy_order_id
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::Database)?
            .flatten();

            let matched_amount = trade.quantity;
            let match_price = trade.price;

            if let Some(epoch_id) = epoch_id {
                sqlx::query(
                    r#"
                    INSERT INTO order_matches (
                        id, epoch_id, buy_order_id, sell_order_id,
                        matched_amount, match_price, match_time, status
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                )
                .bind(trade_id)
                .bind(epoch_id)
                .bind(trade.buy_order_id)
                .bind(trade.sell_order_id)
                .bind(matched_amount)
                .bind(match_price)
                .bind(trade.matched_at)
                .bind("pending")
                .execute(&mut *tx)
                .await
                .map_err(ApiError::Database)?;
            } else {
                warn!(
                    "⚠️  Order {} has no epoch_id, skipping trade persistence",
                    trade.buy_order_id
                );
                tx.rollback().await.map_err(ApiError::Database)?;
                continue;
            }

            // Update buy order with proper partial fill handling
            let buy_result = sqlx::query(
                r#"
                UPDATE trading_orders
                SET filled_amount = filled_amount + $1,
                    status = CASE
                        WHEN filled_amount + $1 >= energy_amount THEN 'filled'::order_status
                        ELSE 'partially_filled'::order_status
                    END,
                    filled_at = CASE
                        WHEN filled_amount + $1 >= energy_amount THEN NOW()
                        ELSE filled_at
                    END,
                    updated_at = NOW()
                WHERE id = $2
                  AND status IN ('pending'::order_status, 'partially_filled'::order_status)
                RETURNING id, energy_amount, filled_amount + $1 as new_filled_amount
                "#,
            )
            .bind(matched_amount)
            .bind(trade.buy_order_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::Database)?;

            if buy_result.is_none() {
                tx.rollback().await.map_err(ApiError::Database)?;
                warn!(
                    "⚠️  Buy order {} not found or already filled, rolling back trade {}",
                    trade.buy_order_id, trade_id
                );
                continue;
            }

            // Update sell order with proper partial fill handling
            let sell_result = sqlx::query(
                r#"
                UPDATE trading_orders
                SET filled_amount = filled_amount + $1,
                    status = CASE
                        WHEN filled_amount + $1 >= energy_amount THEN 'filled'::order_status
                        ELSE 'partially_filled'::order_status
                    END,
                    filled_at = CASE
                        WHEN filled_amount + $1 >= energy_amount THEN NOW()
                        ELSE filled_at
                    END,
                    updated_at = NOW()
                WHERE id = $2
                  AND status IN ('pending'::order_status, 'partially_filled'::order_status)
                RETURNING id, energy_amount, filled_amount + $1 as new_filled_amount
                "#,
            )
            .bind(matched_amount)
            .bind(trade.sell_order_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::Database)?;

            if sell_result.is_none() {
                tx.rollback().await.map_err(ApiError::Database)?;
                warn!(
                    "⚠️  Sell order {} not found or already filled, rolling back trade {}",
                    trade.sell_order_id, trade_id
                );
                continue;
            }

            // Commit transaction
            tx.commit().await.map_err(ApiError::Database)?;
            persisted += 1;

            info!(
                "💾 Persisted trade {}: {} kWh at ${} (buy: {}, sell: {})",
                trade_id, trade.quantity, trade.price, trade.buy_order_id, trade.sell_order_id
            );
        }

        // Update Redis cache after all database updates
        if persisted > 0 {
            if let Some(book) = book_snapshot_for_cache {
                if let Err(e) = self.save_order_book_snapshot(book).await {
                    warn!(
                        "⚠️  Failed to update Redis cache after persisting matches: {}",
                        e
                    );
                }
            }
        }

        Ok(persisted)
    }
}
