use anyhow::Result;
use sqlx::types::BigDecimal;
use sqlx::PgPool;
use sqlx::Row;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::WebSocketService;

/// Background service that automatically matches orders with offers
#[derive(Clone)]
pub struct OrderMatchingEngine {
    db: PgPool,
    running: Arc<RwLock<bool>>,
    match_interval_secs: u64,
    websocket_service: Option<WebSocketService>,
}

impl OrderMatchingEngine {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            running: Arc::new(RwLock::new(false)),
            match_interval_secs: 5, // Check every 5 seconds
            websocket_service: None,
        }
    }

    /// Set the WebSocket service for broadcasting match events
    pub fn with_websocket(mut self, ws_service: WebSocketService) -> Self {
        self.websocket_service = Some(ws_service);
        self
    }

    /// Start the background matching engine
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            warn!("Order matching engine is already running");
            return;
        }
        *running = true;
        drop(running);

        info!("🚀 Starting automated order matching engine (interval: {}s)", self.match_interval_secs);

        let engine = self.clone();
        tokio::spawn(async move {
            engine.run_matching_loop().await;
        });
    }

    /// Stop the background matching engine
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("⏹️  Stopped automated order matching engine");
    }

    /// Main matching loop
    async fn run_matching_loop(&self) {
        loop {
            // Check if we should continue running
            {
                let running = self.running.read().await;
                if !*running {
                    break;
                }
            }

            // Run one matching cycle
            match self.match_orders_cycle().await {
                Ok(matches) => {
                    if matches > 0 {
                        info!("✅ Matching cycle completed: {} new transactions created", matches);
                    } else {
                        debug!("Matching cycle completed: no new matches");
                    }
                }
                Err(e) => {
                    error!("❌ Error in matching cycle: {}", e);
                }
            }

            // Sleep before next cycle
            sleep(Duration::from_secs(self.match_interval_secs)).await;
        }

        info!("Order matching loop terminated");
    }

    /// Run one matching cycle
    async fn match_orders_cycle(&self) -> Result<usize> {
        debug!("Running order matching cycle...");

        // Get all pending buy orders
        let buy_orders = sqlx::query(
            r#"
            SELECT 
                id, 
                user_id, 
                energy_amount, 
                price_per_kwh,
                filled_amount,
                epoch_id
            FROM trading_orders
            WHERE order_type = 'buy' AND status = 'pending'
            ORDER BY created_at ASC
            "#
        )
        .fetch_all(&self.db)
        .await?;

        // Get all pending sell orders
        let sell_orders = sqlx::query(
            r#"
            SELECT 
                id, 
                user_id, 
                energy_amount, 
                price_per_kwh,
                filled_amount,
                epoch_id
            FROM trading_orders
            WHERE order_type = 'sell' AND status = 'pending'
            ORDER BY price_per_kwh ASC, created_at ASC
            "#
        )
        .fetch_all(&self.db)
        .await?;

        if buy_orders.is_empty() || sell_orders.is_empty() {
            return Ok(0);
        }

        debug!("Found {} buy orders and {} sell orders to process", buy_orders.len(), sell_orders.len());

        let mut matches_created = 0;

        // Try to match each buy order with sell orders
        for buy_order in &buy_orders {
            let buy_order_id: Uuid = buy_order.try_get("id")?;
            let buyer_id: Uuid = buy_order.try_get("user_id")?;
            let buy_energy_amount: BigDecimal = buy_order.try_get("energy_amount")?;
            let buy_filled_amount: BigDecimal = buy_order.try_get("filled_amount")?;
            let buy_price_per_kwh: BigDecimal = buy_order.try_get("price_per_kwh")?;
            let epoch_id: Uuid = buy_order.try_get("epoch_id")?;

            // Calculate remaining amount needed
            let remaining_buy_amount = &buy_energy_amount - &buy_filled_amount;
            let zero = BigDecimal::from_str("0").unwrap();
            if remaining_buy_amount <= zero {
                continue; // Order already fully filled
            }

            // Find compatible sell orders (price <= buy price)
            for sell_order in &sell_orders {
                let sell_order_id: Uuid = sell_order.try_get("id")?;
                let seller_id: Uuid = sell_order.try_get("user_id")?;
                let sell_energy_amount: BigDecimal = sell_order.try_get("energy_amount")?;
                let sell_filled_amount: BigDecimal = sell_order.try_get("filled_amount")?;
                let sell_price_per_kwh: BigDecimal = sell_order.try_get("price_per_kwh")?;
                let sell_epoch_id: Uuid = sell_order.try_get("epoch_id")?;

                // Check if sell order is compatible
                if sell_price_per_kwh > buy_price_per_kwh {
                    continue; // Sell price too high
                }

                if sell_epoch_id != epoch_id {
                    continue; // Different epochs
                }

                // Calculate remaining amount available to sell
                let remaining_sell_amount = &sell_energy_amount - &sell_filled_amount;
                if remaining_sell_amount <= zero {
                    continue; // Sell order already fully filled
                }

                // Calculate match amount (min of remaining buy and sell amounts)
                let match_amount = if remaining_buy_amount < remaining_sell_amount {
                    remaining_buy_amount.clone()
                } else {
                    remaining_sell_amount.clone()
                };

                let match_price = sell_price_per_kwh.clone(); // Use sell price (market maker advantage)
                let total_price = &match_amount * &match_price;

                debug!(
                    "Matching buy order {} with sell order {}: {} kWh at ${}/kWh (total: ${})",
                    buy_order_id, sell_order_id, match_amount, match_price, total_price
                );

                // Create order match
                match self
                    .create_order_match(
                        epoch_id,
                        buy_order_id,
                        sell_order_id,
                        buyer_id,
                        seller_id,
                        match_amount.clone(),
                        match_price.clone(),
                        total_price.clone(),
                    )
                    .await
                {
                    Ok(match_id) => {
                        info!(
                            "✅ Created match {}: {} kWh from sell order {} to buy order {} at ${}/kWh",
                            match_id, match_amount, sell_order_id, buy_order_id, match_price
                        );
                        matches_created += 1;

                        // Update buy order filled amount
                        let new_buy_filled = &buy_filled_amount + &match_amount;
                        let buy_complete = new_buy_filled >= buy_energy_amount;
                        
                        sqlx::query(
                            r#"
                            UPDATE trading_orders 
                            SET filled_amount = $1, 
                                status = CASE WHEN $2 THEN 'filled' ELSE 'active' END,
                                updated_at = NOW()
                            WHERE id = $3
                            "#,
                        )
                        .bind(&new_buy_filled)
                        .bind(buy_complete)
                        .bind(buy_order_id)
                        .execute(&self.db)
                        .await?;

                        // Update sell order filled amount
                        let new_sell_filled = &sell_filled_amount + &match_amount;
                        let sell_complete = new_sell_filled >= sell_energy_amount;
                        
                        sqlx::query(
                            r#"
                            UPDATE trading_orders 
                            SET filled_amount = $1, 
                                status = CASE WHEN $2 THEN 'filled' ELSE 'active' END,
                                updated_at = NOW()
                            WHERE id = $3
                            "#,
                        )
                        .bind(&new_sell_filled)
                        .bind(sell_complete)
                        .bind(sell_order_id)
                        .execute(&self.db)
                        .await?;

                        if buy_complete {
                            debug!("Buy order {} fully filled", buy_order_id);
                            break; // Move to next buy order
                        }
                    }
                    Err(e) => {
                        error!("Failed to create order match: {}", e);
                        continue;
                    }
                }
            }
        }

        Ok(matches_created)
    }

    /// Create an order match record
    async fn create_order_match(
        &self,
        epoch_id: Uuid,
        buy_order_id: Uuid,
        sell_order_id: Uuid,
        buyer_id: Uuid,
        seller_id: Uuid,
        energy_amount: BigDecimal,
        price_per_kwh: BigDecimal,
        total_price: BigDecimal,
    ) -> Result<Uuid> {
        let match_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO order_matches (
                id,
                epoch_id,
                buy_order_id,
                sell_order_id,
                matched_amount,
                match_price,
                match_time,
                status,
                created_at,
                updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, NOW(), 'pending', NOW(), NOW())
            "#,
        )
        .bind(match_id)
        .bind(epoch_id)
        .bind(buy_order_id)
        .bind(sell_order_id)
        .bind(&energy_amount)
        .bind(&price_per_kwh)
        .execute(&self.db)
        .await?;

        // Broadcast order matched event via WebSocket
        if let Some(ws_service) = &self.websocket_service {
            let energy_f64 = energy_amount.to_string().parse::<f64>().unwrap_or(0.0);
            let price_f64 = price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0);
            
            tokio::spawn({
                let ws = ws_service.clone();
                let mid = match_id.to_string();
                let buy_id = buy_order_id.to_string();
                let sell_id = sell_order_id.to_string();
                async move {
                    ws.broadcast_order_matched(buy_id, sell_id, mid, energy_f64, price_f64).await;
                }
            });
        }

        Ok(match_id)
    }

    /// Manually trigger a matching cycle (for testing or API endpoints)
    pub async fn trigger_matching(&self) -> Result<usize> {
        info!("Manual matching trigger requested");
        self.match_orders_cycle().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        // This is a placeholder test since we need a real database for full testing
        // In production, you would use a test database
        let pool = PgPool::connect_lazy("postgresql://localhost/test").unwrap();
        let engine = OrderMatchingEngine::new(pool);
        assert_eq!(engine.match_interval_secs, 5);
    }
}
