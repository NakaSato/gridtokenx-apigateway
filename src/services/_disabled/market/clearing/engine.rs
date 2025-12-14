//! Market Clearing Engine implementation
//!
//! This module contains the main MarketClearingEngine struct and all
//! clearing/matching logic for P2P energy trading.

use crate::error::Result; // ApiError implicit in Result
use crate::services::settlement::SettlementService;
use crate::services::WebSocketService;

use super::super::order_book::OrderBook;
use super::super::types::{ClearingPrice, OrderBookSnapshot, TradeMatch};
use super::matching::MatchingEngine;
use super::persistence::ClearingPersistence;

use chrono::Utc;
// use redis::AsyncCommands; // Unused
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct MarketClearingEngine {
    persistence: Arc<ClearingPersistence>,
    order_book: Arc<RwLock<OrderBook>>,
    websocket: Option<WebSocketService>,
    settlement: Option<SettlementService>,
}

impl MarketClearingEngine {
    pub fn new(db: PgPool, redis: redis::Client) -> Self {
        Self {
            persistence: Arc::new(ClearingPersistence::new(db, redis)),
            order_book: Arc::new(RwLock::new(OrderBook::new())),
            websocket: None,
            settlement: None,
        }
    }

    /// Set WebSocket service for real-time broadcasts
    pub fn with_websocket(mut self, websocket: WebSocketService) -> Self {
        self.websocket = Some(websocket);
        self
    }

    /// Set settlement service for blockchain integration
    pub fn with_settlement(mut self, settlement: SettlementService) -> Self {
        self.settlement = Some(settlement);
        self
    }

    /// Save order book snapshot to Redis
    pub async fn save_order_book_snapshot(&self) -> Result<()> {
        let book = self.order_book.read().await;
        self.persistence.save_order_book_snapshot(&book).await
    }

    /// Restore order book from Redis
    pub async fn restore_order_book_from_redis(&self) -> Result<usize> {
        let mut book = self.order_book.write().await;
        self.persistence
            .restore_order_book_from_redis(&mut book)
            .await
    }

    /// Clear Redis order book cache
    pub async fn clear_redis_cache(&self) -> Result<()> {
        self.persistence.clear_redis_cache().await
    }

    /// Load active orders from database into order book
    pub async fn load_order_book(&self) -> Result<usize> {
        let mut book = self.order_book.write().await;
        self.persistence.load_order_book(&mut book).await
    }

    /// Calculate market clearing price using supply-demand curves
    pub async fn calculate_clearing_price(&self) -> Option<ClearingPrice> {
        let book = self.order_book.read().await;
        MatchingEngine::calculate_clearing_price(&book)
    }

    /// Match orders at market clearing price with atomic partial fill handling
    pub async fn match_orders(&self) -> Result<Vec<TradeMatch>> {
        let mut book = self.order_book.write().await;

        let (matches, expired_orders) = MatchingEngine::match_orders(&mut book)?;

        // Handle expired orders persistence
        if !expired_orders.is_empty() {
            self.persistence
                .mark_orders_as_expired(&expired_orders)
                .await;
        }

        // Save updated order book to Redis if there were matches
        if !matches.is_empty() {
            // We need to release the write lock before saving if save_snapshot takes a read lock?
            // save_order_book_snapshot takes &OrderBook.
            // Since we have &mut reference here, we can pass it directly.
            // But persistence logic expects &OrderBook.
            // It's safe to pass properties.

            // Wait, save_order_book_snapshot implementation takes &OrderBook.
            // `book` here is `RwLockWriteGuard<OrderBook>`. Dereferencing gives `&mut OrderBook`, which coerces to `&OrderBook`.
            // So we can call:
            if let Err(e) = self.persistence.save_order_book_snapshot(&book).await {
                warn!("⚠️  Failed to save order book to Redis: {}", e);
            }
        }

        Ok(matches)
    }

    /// Execute a complete matching cycle: match orders and persist results
    pub async fn execute_matching_cycle(&self) -> Result<usize> {
        info!("🔄 Starting matching cycle");

        // Load active orders (refreshes book from DB/Redis)
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
        // We pass a snapshot for cache update if needed.
        // Actually match_orders already updated the cache for the in-memory state changes.
        // But persist_matches updates the DB state (partially filled/filled statuses).
        // The in-memory book is already updated in match_orders.
        // The persistence layer might want to re-save the book?
        // Let's pass the book just in case, but we need a read lock now.
        let book = self.order_book.read().await;
        let persisted = self
            .persistence
            .persist_matches(matches.clone(), Some(&book))
            .await?;
        drop(book);

        // Execute settlements for matched trades if settlement service is available
        if let Some(settlement) = &self.settlement {
            info!("🔄 Creating settlements for {} trades", matches.len());
            if let Err(e) = settlement
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
    pub async fn broadcast_market_depth(&self, ws: &WebSocketService) {
        let snapshot = self.get_order_book_snapshot().await;

        let total_buy_volume: rust_decimal::Decimal =
            snapshot.buy_depth.iter().map(|(_, vol)| vol).sum();

        let total_sell_volume: rust_decimal::Decimal =
            snapshot.sell_depth.iter().map(|(_, vol)| vol).sum();

        let spread_percentage = match (&snapshot.best_bid, &snapshot.best_ask) {
            (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => Some(
                ((*ask - *bid) / *bid * rust_decimal::Decimal::from(100))
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
