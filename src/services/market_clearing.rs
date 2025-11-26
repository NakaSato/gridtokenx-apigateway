// Market Clearing Engine for P2P Energy Trading
// Implements double auction mechanism with price discovery

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::WebSocketService;
use crate::services::settlement_service::SettlementService;
use tracing::error;

/// Order side (Buy or Sell)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub epoch_id: Option<Uuid>,
    pub side: OrderSide,
    pub energy_amount: Decimal, // kWh
    pub price: Decimal,         // USD per kWh
    pub filled_amount: Decimal, // kWh already filled
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl BookOrder {
    /// Remaining unfilled amount
    pub fn remaining_amount(&self) -> Decimal {
        self.energy_amount - self.filled_amount
    }

    /// Check if order is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if order is fully filled
    pub fn is_filled(&self) -> bool {
        self.filled_amount >= self.energy_amount
    }
}

/// Price level in the order book
#[derive(Debug, Clone)]
struct PriceLevel {
    price: Decimal,
    total_volume: Decimal,
    orders: Vec<BookOrder>,
}

impl PriceLevel {
    fn new(price: Decimal) -> Self {
        Self {
            price,
            total_volume: Decimal::ZERO,
            orders: Vec::new(),
        }
    }

    fn add_order(&mut self, order: BookOrder) {
        self.total_volume += order.remaining_amount();
        self.orders.push(order);
    }

    fn remove_order(&mut self, order_id: &Uuid) -> Option<BookOrder> {
        if let Some(pos) = self.orders.iter().position(|o| &o.id == order_id) {
            let order = self.orders.remove(pos);
            self.total_volume -= order.remaining_amount();
            Some(order)
        } else {
            None
        }
    }
}

/// Order book with buy and sell sides
#[derive(Debug, Clone)]
pub struct OrderBook {
    // Buy orders sorted by price (descending) - highest bids first
    buy_levels: BTreeMap<String, PriceLevel>, // String key for decimal sorting
    // Sell orders sorted by price (ascending) - lowest asks first
    sell_levels: BTreeMap<String, PriceLevel>,
    // Quick lookup by order ID
    order_index: HashMap<Uuid, OrderSide>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            buy_levels: BTreeMap::new(),
            sell_levels: BTreeMap::new(),
            order_index: HashMap::new(),
        }
    }

    /// Add order to the book
    pub fn add_order(&mut self, order: BookOrder) {
        let price_key = Self::price_key(order.price);

        match order.side {
            OrderSide::Buy => {
                let level = self
                    .buy_levels
                    .entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                level.add_order(order.clone());
            }
            OrderSide::Sell => {
                let level = self
                    .sell_levels
                    .entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                level.add_order(order.clone());
            }
        }

        self.order_index.insert(order.id, order.side);
    }

    /// Remove order from the book
    pub fn remove_order(&mut self, order_id: &Uuid) -> Option<BookOrder> {
        let side = self.order_index.remove(order_id)?;

        let mut order_removed = None;

        match side {
            OrderSide::Buy => {
                // Search through buy levels
                let mut empty_price_key = None;
                for (price_key, level) in self.buy_levels.iter_mut() {
                    if let Some(order) = level.remove_order(order_id) {
                        order_removed = Some(order);
                        if level.orders.is_empty() {
                            empty_price_key = Some(price_key.clone());
                        }
                        break;
                    }
                }
                if let Some(key) = empty_price_key {
                    self.buy_levels.remove(&key);
                }
            }
            OrderSide::Sell => {
                // Search through sell levels
                let mut empty_price_key = None;
                for (price_key, level) in self.sell_levels.iter_mut() {
                    if let Some(order) = level.remove_order(order_id) {
                        order_removed = Some(order);
                        if level.orders.is_empty() {
                            empty_price_key = Some(price_key.clone());
                        }
                        break;
                    }
                }
                if let Some(key) = empty_price_key {
                    self.sell_levels.remove(&key);
                }
            }
        }

        order_removed
    }

    /// Get best bid (highest buy price)
    pub fn best_bid(&self) -> Option<Decimal> {
        self.buy_levels
            .iter()
            .next_back()
            .map(|(_, level)| level.price)
    }

    /// Get best ask (lowest sell price)
    pub fn best_ask(&self) -> Option<Decimal> {
        self.sell_levels.iter().next().map(|(_, level)| level.price)
    }

    /// Calculate mid-market price
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Get spread (difference between best ask and best bid)
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get total volume at each price level for buy side
    pub fn buy_depth(&self) -> Vec<(Decimal, Decimal)> {
        self.buy_levels
            .iter()
            .rev() // Highest prices first
            .map(|(_, level)| (level.price, level.total_volume))
            .collect()
    }

    /// Get total volume at each price level for sell side
    pub fn sell_depth(&self) -> Vec<(Decimal, Decimal)> {
        self.sell_levels
            .iter()
            .map(|(_, level)| (level.price, level.total_volume))
            .collect()
    }

    /// Convert price to sortable string key
    fn price_key(price: Decimal) -> String {
        // Pad with zeros for proper string sorting
        format!("{:020.8}", price)
    }

    /// Clear expired orders
    pub fn remove_expired_orders(&mut self) -> Vec<Uuid> {
        let mut expired_ids = Vec::new();

        // Find expired buy orders
        for level in self.buy_levels.values_mut() {
            let before_count = level.orders.len();
            level.orders.retain(|order| {
                if order.is_expired() {
                    expired_ids.push(order.id);
                    false
                } else {
                    true
                }
            });
            if level.orders.len() < before_count {
                level.total_volume = level.orders.iter().map(|o| o.remaining_amount()).sum();
            }
        }

        // Find expired sell orders
        for level in self.sell_levels.values_mut() {
            let before_count = level.orders.len();
            level.orders.retain(|order| {
                if order.is_expired() {
                    expired_ids.push(order.id);
                    false
                } else {
                    true
                }
            });
            if level.orders.len() < before_count {
                level.total_volume = level.orders.iter().map(|o| o.remaining_amount()).sum();
            }
        }

        // Clean up empty levels
        self.buy_levels.retain(|_, level| !level.orders.is_empty());
        self.sell_levels.retain(|_, level| !level.orders.is_empty());

        // Update index
        for id in &expired_ids {
            self.order_index.remove(id);
        }

        expired_ids
    }

    /// Clear all orders from the book
    pub fn clear(&mut self) {
        self.buy_levels.clear();
        self.sell_levels.clear();
        self.order_index.clear();
    }
}

/// Trade match result
#[derive(Debug, Clone, Serialize)]
pub struct TradeMatch {
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    pub price: Decimal,
    pub quantity: Decimal,
    pub total_value: Decimal,
    pub matched_at: DateTime<Utc>,
    pub epoch_id: Uuid,
}

/// Market clearing price calculation result
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClearingPrice {
    #[schema(value_type = String)]
    pub price: Decimal,
    #[schema(value_type = String)]
    pub volume: Decimal,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
}

/// Market Clearing Engine
#[derive(Clone)]
pub struct MarketClearingEngine {
    db: PgPool,
    redis: redis::Client,
    order_book: Arc<RwLock<OrderBook>>,
    websocket: Option<WebSocketService>,
    settlement_service: Option<SettlementService>,
}

impl MarketClearingEngine {
    pub fn new(db: PgPool, redis: redis::Client) -> Self {
        Self {
            db,
            redis,
            order_book: Arc::new(RwLock::new(OrderBook::new())),
            websocket: None,
            settlement_service: None,
        }
    }

    /// Set WebSocket service for real-time broadcasts
    pub fn with_websocket(mut self, websocket: WebSocketService) -> Self {
        self.websocket = Some(websocket);
        self
    }

    /// Set settlement service for blockchain integration
    pub fn with_settlement_service(mut self, settlement_service: SettlementService) -> Self {
        self.settlement_service = Some(settlement_service);
        self
    }

    /// Save order book snapshot to Redis
    async fn save_order_book_snapshot(&self) -> Result<(), ApiError> {
        let book = self.order_book.read().await;
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {}", e)))?;

        // Save each buy order to Redis sorted set (score = price for ordering)
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
    async fn restore_order_book_from_redis(&self) -> Result<usize, ApiError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {}", e)))?;

        let mut book = self.order_book.write().await;
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
    async fn clear_redis_cache(&self) -> Result<(), ApiError> {
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
    /// First attempts to restore from Redis cache, then falls back to database
    pub async fn load_order_book(&self) -> Result<usize, ApiError> {
        // Try to restore from Redis cache first
        match self.restore_order_book_from_redis().await {
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

        let mut book = self.order_book.write().await;
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

            let energy_amount = Decimal::from_str_exact(&energy_str)
                .map_err(|_| ApiError::Internal("Invalid energy amount".into()))?;
            let price = Decimal::from_str_exact(&price_str)
                .map_err(|_| ApiError::Internal("Invalid price".into()))?;
            let filled_amount = Decimal::from_str_exact(&filled_str)
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

        drop(book); // Release lock before saving to Redis

        info!("📚 Loaded {} active orders from database", loaded_count);

        // Save to Redis for future quick restores
        if loaded_count > 0 {
            if let Err(e) = self.save_order_book_snapshot().await {
                warn!("Failed to save order book to Redis: {}", e);
            }
        }

        Ok(loaded_count)
    }

    /// Calculate market clearing price using supply-demand curves
    pub async fn calculate_clearing_price(&self) -> Option<ClearingPrice> {
        let book = self.order_book.read().await;

        let buy_depth = book.buy_depth();
        let sell_depth = book.sell_depth();

        if buy_depth.is_empty() || sell_depth.is_empty() {
            return None;
        }

        // Build cumulative supply and demand curves
        let mut demand_curve: Vec<(Decimal, Decimal)> = Vec::new();
        let mut cumulative_demand = Decimal::ZERO;
        for (price, volume) in buy_depth {
            cumulative_demand += volume;
            demand_curve.push((price, cumulative_demand));
        }

        let mut supply_curve: Vec<(Decimal, Decimal)> = Vec::new();
        let mut cumulative_supply = Decimal::ZERO;
        for (price, volume) in sell_depth {
            cumulative_supply += volume;
            supply_curve.push((price, cumulative_supply));
        }

        // Find intersection point (clearing price)
        // This is where supply meets or exceeds demand
        let mut best_clearing: Option<ClearingPrice> = None;
        let mut max_volume = Decimal::ZERO;

        for (demand_price, demand_vol) in &demand_curve {
            for (supply_price, supply_vol) in &supply_curve {
                // Can only clear if buyers willing to pay >= sellers asking
                if demand_price >= supply_price {
                    let clearable_volume = (*demand_vol).min(*supply_vol);

                    if clearable_volume > max_volume {
                        max_volume = clearable_volume;
                        // Clearing price is midpoint of bid-ask spread
                        let clearing_price = (*demand_price + *supply_price) / Decimal::TWO;

                        best_clearing = Some(ClearingPrice {
                            price: clearing_price,
                            volume: clearable_volume,
                            buy_orders_count: demand_curve.len(),
                            sell_orders_count: supply_curve.len(),
                        });
                    }
                }
            }
        }

        best_clearing
    }

    /// Match orders at market clearing price with atomic partial fill handling
    pub async fn match_orders(&self) -> Result<Vec<TradeMatch>, ApiError> {
        let mut matches = Vec::new();
        let mut book = self.order_book.write().await;

        // Remove expired orders first
        let expired = book.remove_expired_orders();
        if !expired.is_empty() {
            info!("🗑️  Removed {} expired orders", expired.len());
            // Update database to mark as expired
            for order_id in expired {
                let _ = sqlx::query("UPDATE trading_orders SET status = 'expired' WHERE id = $1")
                    .bind(order_id)
                    .execute(&self.db)
                    .await;
            }
        }

        // Continuous matching loop until no more crosses exist
        loop {
            let best_bid = book.best_bid();
            let best_ask = book.best_ask();

            match (best_bid, best_ask) {
                (Some(bid), Some(ask)) if bid >= ask => {
                    // There's overlap - we can match orders
                    debug!("🔄 Market crossover: Bid ${} >= Ask ${}", bid, ask);

                    // Get the best sell order (lowest ask)
                    let sell_order = {
                        let (_, sell_level) = book
                            .sell_levels
                            .iter_mut()
                            .next()
                            .ok_or(ApiError::Internal("No sell orders available".into()))?;

                        if sell_level.orders.is_empty() {
                            break; // No more sell orders
                        }

                        sell_level.orders[0].clone()
                    };

                    if sell_order.is_filled() || sell_order.is_expired() {
                        // Remove filled/expired order and continue
                        book.remove_order(&sell_order.id);
                        continue;
                    }

                    // Get the best buy order (highest bid)
                    let buy_order = {
                        let (_, buy_level) = book
                            .buy_levels
                            .iter_mut()
                            .rev()
                            .next()
                            .ok_or(ApiError::Internal("No buy orders available".into()))?;

                        if buy_level.orders.is_empty() {
                            break; // No more buy orders
                        }

                        buy_level.orders[0].clone()
                    };

                    if buy_order.is_filled() || buy_order.is_expired() {
                        // Remove filled/expired order and continue
                        book.remove_order(&buy_order.id);
                        continue;
                    }

                    // Verify orders can still match
                    if buy_order.price < sell_order.price {
                        break; // No more matches possible
                    }

                    // Calculate match quantity (minimum of remaining amounts)
                    let sell_remaining = sell_order.remaining_amount();
                    let buy_remaining = buy_order.remaining_amount();
                    let match_quantity = sell_remaining.min(buy_remaining);

                    if match_quantity <= Decimal::ZERO {
                        break;
                    }

                    // Execution price is midpoint of bid-ask spread
                    let execution_price = (buy_order.price + sell_order.price) / Decimal::TWO;
                    let total_value = match_quantity * execution_price;

                    // Create trade match
                    let trade = TradeMatch {
                        buy_order_id: buy_order.id,
                        sell_order_id: sell_order.id,
                        buyer_id: buy_order.user_id,
                        seller_id: sell_order.user_id,
                        price: execution_price,
                        quantity: match_quantity,
                        total_value,
                        matched_at: Utc::now(),
                        epoch_id: buy_order
                            .epoch_id
                            .or(sell_order.epoch_id)
                            .unwrap_or_else(Uuid::new_v4),
                    };

                    info!(
                        "✅ Matched: {} kWh at ${}/kWh (buyer: {}, seller: {})",
                        match_quantity, execution_price, buy_order.user_id, sell_order.user_id
                    );

                    // Update order filled amounts in-memory (atomic update)
                    self.update_order_filled_amount_in_book(
                        &mut book,
                        &buy_order.id,
                        match_quantity,
                    )?;
                    self.update_order_filled_amount_in_book(
                        &mut book,
                        &sell_order.id,
                        match_quantity,
                    )?;

                    // Remove fully filled orders from book
                    if buy_order.remaining_amount() + match_quantity >= buy_order.energy_amount {
                        book.remove_order(&buy_order.id);
                        debug!("Removed fully filled buy order: {}", buy_order.id);
                    }
                    if sell_order.remaining_amount() + match_quantity >= sell_order.energy_amount {
                        book.remove_order(&sell_order.id);
                        debug!("Removed fully filled sell order: {}", sell_order.id);
                    }

                    matches.push(trade);
                }
                (Some(bid), Some(ask)) => {
                    debug!("No market crossover: Bid ${} < Ask ${}", bid, ask);
                    break;
                }
                _ => {
                    debug!("Insufficient market depth for matching");
                    break;
                }
            }
        }

        // Save updated order book to Redis
        if !matches.is_empty() {
            drop(book); // Release lock before Redis operation
            if let Err(e) = self.save_order_book_snapshot().await {
                warn!("⚠️  Failed to save order book to Redis: {}", e);
            }
        }

        Ok(matches)
    }

    /// Update order filled amount in the order book (in-memory only)
    fn update_order_filled_amount_in_book(
        &self,
        book: &mut OrderBook,
        order_id: &Uuid,
        amount: Decimal,
    ) -> Result<(), ApiError> {
        // Find the order in the book and update its filled amount
        let side = book
            .order_index
            .get(order_id)
            .ok_or(ApiError::NotFound("Order not found in book".into()))?;

        let levels = match side {
            OrderSide::Buy => &mut book.buy_levels,
            OrderSide::Sell => &mut book.sell_levels,
        };

        for (_, level) in levels.iter_mut() {
            for order in level.orders.iter_mut() {
                if &order.id == order_id {
                    order.filled_amount += amount;
                    level.total_volume -= amount;
                    return Ok(());
                }
            }
        }

        Err(ApiError::NotFound("Order not found in price level".into()))
    }

    /// Persist matched trades to database with atomic order updates
    pub async fn persist_matches(&self, matches: Vec<TradeMatch>) -> Result<usize, ApiError> {
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

            // Convert Decimal to BigDecimal for database
            let matched_amount_bd = sqlx::types::BigDecimal::from_str(&trade.quantity.to_string())
                .map_err(|e| ApiError::Internal(format!("Invalid quantity: {}", e)))?;
            let match_price_bd = sqlx::types::BigDecimal::from_str(&trade.price.to_string())
                .map_err(|e| ApiError::Internal(format!("Invalid price: {}", e)))?;

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
                .bind(matched_amount_bd.clone())
                .bind(match_price_bd.clone())
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
            .bind(matched_amount_bd.clone())
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
            .bind(matched_amount_bd.clone())
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
            if let Err(e) = self.save_order_book_snapshot().await {
                warn!(
                    "⚠️  Failed to update Redis cache after persisting matches: {}",
                    e
                );
            }
        }

        Ok(persisted)
    }

    /// Execute a complete matching cycle: match orders and persist results
    pub async fn execute_matching_cycle(&self) -> Result<usize, ApiError> {
        info!("🔄 Starting matching cycle");

        // Load active orders from database
        self.load_order_book().await?;

        // Broadcast order book snapshot before matching
        if let Some(ws) = &self.websocket {
            self.broadcast_order_book_snapshot(ws).await;
        }

        // Match orders in-memory
        let matches = self.match_orders().await?;

        if matches.is_empty() {
            debug!("No matches found in this cycle");
            return Ok(0);
        }

        info!("Found {} matches, persisting to database", matches.len());

        // Broadcast trade executions
        if let Some(ws) = &self.websocket {
            for trade in &matches {
                ws.broadcast_trade_executed(
                    Uuid::new_v4().to_string(), // Trade ID
                    trade.buy_order_id.to_string(),
                    trade.sell_order_id.to_string(),
                    trade.buyer_id.to_string(),
                    trade.seller_id.to_string(),
                    trade.quantity.to_string(),
                    trade.price.to_string(),
                    trade.total_value.to_string(),
                    chrono::Utc::now().to_string(),
                )
                .await;
            }
        }

        // Persist to database with atomic updates
        let persisted = self.persist_matches(matches.clone()).await?;

        // Execute settlements for matched trades if settlement service is available
        if let Some(settlement_service) = &self.settlement_service {
            info!("🔄 Creating settlements for {} trades", matches.len());
            if let Err(e) = settlement_service
                .create_settlements_from_trades(matches)
                .await
            {
                error!("❌ Failed to create settlements: {}", e);
            }
        }

        // Broadcast updated order book after matching
        if let Some(ws) = &self.websocket {
            self.broadcast_order_book_snapshot(ws).await;
            self.broadcast_market_depth(ws).await;
        }

        info!("✅ Matching cycle complete: {} trades persisted", persisted);
        Ok(persisted)
    }

    /// Broadcast order book snapshot to WebSocket clients
    async fn broadcast_order_book_snapshot(&self, ws: &WebSocketService) {
        let snapshot = self.get_order_book_snapshot().await;

        let bids: Vec<(String, String)> = snapshot
            .buy_depth
            .iter()
            .map(|(price, volume)| (price.to_string(), volume.to_string()))
            .collect();

        let asks: Vec<(String, String)> = snapshot
            .sell_depth
            .iter()
            .map(|(price, volume)| (price.to_string(), volume.to_string()))
            .collect();

        ws.broadcast_order_book_snapshot(
            bids,
            asks,
            snapshot.best_bid.map(|p| p.to_string()),
            snapshot.best_ask.map(|p| p.to_string()),
            snapshot.mid_price.map(|p| p.to_string()),
            snapshot.spread.map(|p| p.to_string()),
        )
        .await;
    }

    /// Broadcast market depth update to WebSocket clients
    async fn broadcast_market_depth(&self, ws: &WebSocketService) {
        let snapshot = self.get_order_book_snapshot().await;

        let total_buy_volume: rust_decimal::Decimal =
            snapshot.buy_depth.iter().map(|(_, vol)| vol).sum();

        let total_sell_volume: rust_decimal::Decimal =
            snapshot.sell_depth.iter().map(|(_, vol)| vol).sum();

        let spread_percentage = match (&snapshot.best_bid, &snapshot.best_ask) {
            (Some(bid), Some(ask)) if *bid > Decimal::ZERO => Some(
                ((*ask - *bid) / *bid * Decimal::from(100))
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0),
            ),
            _ => None,
        };

        ws.broadcast_market_depth_update(
            total_buy_volume.to_string(),
            total_sell_volume.to_string(),
            snapshot.buy_depth.len(),
            snapshot.sell_depth.len(),
            spread_percentage,
        )
        .await;
    }

    /// Get current order book snapshot
    pub async fn get_order_book_snapshot(&self) -> OrderBookSnapshot {
        let book = self.order_book.read().await;

        OrderBookSnapshot {
            best_bid: book.best_bid(),
            best_ask: book.best_ask(),
            mid_price: book.mid_price(),
            spread: book.spread(),
            buy_depth: book.buy_depth(),
            sell_depth: book.sell_depth(),
            timestamp: Utc::now(),
        }
    }
}

/// Order book snapshot for API responses
#[derive(Debug, Clone, Serialize)]
pub struct OrderBookSnapshot {
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub mid_price: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub buy_depth: Vec<(Decimal, Decimal)>, // (price, volume)
    pub sell_depth: Vec<(Decimal, Decimal)>,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_creation() {
        let book = OrderBook::new();
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
    }

    #[test]
    fn test_add_buy_order() {
        let mut book = OrderBook::new();
        let order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert_eq!(
            book.best_bid(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_price_priority() {
        let mut book = OrderBook::new();

        // Add buy orders at different prices
        let order1 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let order2 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.20").unwrap(), // Higher price
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order1);
        book.add_order(order2);

        // Best bid should be the highest price
        assert_eq!(
            book.best_bid(),
            Some(Decimal::from_str_exact("0.20").unwrap())
        );
    }

    #[test]
    fn test_add_sell_order() {
        let mut book = OrderBook::new();
        let order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert_eq!(
            book.best_ask(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_sell_price_priority() {
        let mut book = OrderBook::new();

        // Add sell orders at different prices
        let order1 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.20").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let order2 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(), // Lower price
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order1);
        book.add_order(order2);

        // Best ask should be the lowest price
        assert_eq!(
            book.best_ask(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_order_removal() {
        let mut book = OrderBook::new();
        let order_id = Uuid::new_v4();
        let order = BookOrder {
            id: order_id,
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert!(book.best_bid().is_some());

        book.remove_order(&order_id);
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_mid_price_calculation() {
        let mut book = OrderBook::new();

        let buy_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.10").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let sell_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.20").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(buy_order);
        book.add_order(sell_order);

        // Mid price should be (0.10 + 0.20) / 2 = 0.15
        assert_eq!(
            book.mid_price(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_spread_calculation() {
        let mut book = OrderBook::new();

        let buy_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.10").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let sell_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(buy_order);
        book.add_order(sell_order);

        // Spread should be 0.15 - 0.10 = 0.05
        assert_eq!(
            book.spread(),
            Some(Decimal::from_str_exact("0.05").unwrap())
        );
    }

    #[test]
    fn test_order_book_depth() {
        let mut book = OrderBook::new();

        // Add multiple buy orders at different prices
        for i in 1..=5 {
            let order = BookOrder {
                id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                side: OrderSide::Buy,
                energy_amount: Decimal::from(100),
                price: Decimal::from_str_exact(&format!("0.{}", i * 10)).unwrap(),
                filled_amount: Decimal::ZERO,
                created_at: Utc::now(),
                epoch_id: None,
                expires_at: Utc::now() + chrono::Duration::hours(24),
            };
            book.add_order(order);
        }

        let depth = book.buy_depth();
        assert_eq!(depth.len(), 5);

        // Verify total volume
        let total_volume: Decimal = depth.iter().map(|(_, v)| v).sum();
        assert_eq!(total_volume, Decimal::from(500));
    }

    #[test]
    fn test_book_order_remaining_amount() {
        let order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::from(30),
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        assert_eq!(order.remaining_amount(), Decimal::from(70));
    }

    #[test]
    fn test_book_order_is_filled() {
        let mut order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::from(100),
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        assert!(order.is_filled());

        order.filled_amount = Decimal::from(99);
        assert!(!order.is_filled());
    }

    #[test]
    fn test_book_order_is_expired() {
        let expired_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            epoch_id: None,
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now() - chrono::Duration::hours(48),
            expires_at: Utc::now() - chrono::Duration::hours(24),
        };

        assert!(expired_order.is_expired());

        let active_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        assert!(!active_order.is_expired());
    }

    #[test]
    fn test_multiple_orders_same_price() {
        let mut book = OrderBook::new();
        let price = Decimal::from_str_exact("0.15").unwrap();

        // Add three buy orders at the same price
        for _ in 0..3 {
            let order = BookOrder {
                id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                side: OrderSide::Buy,
                energy_amount: Decimal::from(100),
                price,
                filled_amount: Decimal::ZERO,
                created_at: Utc::now(),
                epoch_id: None,
                expires_at: Utc::now() + chrono::Duration::hours(24),
            };
            book.add_order(order);
        }

        let depth = book.buy_depth();
        assert_eq!(depth.len(), 1); // One price level
        assert_eq!(depth[0].1, Decimal::from(300)); // Total volume of 300
    }
}
