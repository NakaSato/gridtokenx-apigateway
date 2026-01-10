pub mod types;

use anyhow::Result;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use solana_sdk::pubkey::Pubkey;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::{
    database::schema::types::{OrderStatus, OrderSide},
    services::{market_clearing::{TradeMatch, MarketClearingService}, SettlementService, WebSocketService, GridTopologyService, BlockchainService},
    middleware::metrics::{track_order_matched, track_trading_operation},
};

/// Background service that automatically matches orders with offers
#[derive(Clone)]
pub struct OrderMatchingEngine {
    db: PgPool,
    running: Arc<RwLock<bool>>,
    match_interval_secs: u64,
    websocket_service: Option<WebSocketService>,
    settlement: Option<SettlementService>,
    market_clearing: Option<MarketClearingService>,
    blockchain_service: Option<BlockchainService>,
    grid_topology: GridTopologyService,
}

impl OrderMatchingEngine {
    pub fn new(db: PgPool) -> Self {
        // Read interval from environment variable, default to 5 seconds
        let match_interval_secs = std::env::var("MATCHING_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(5);
        
        if match_interval_secs != 5 {
            info!("Order matching interval set to {} seconds", match_interval_secs);
        }

        Self {
            db,
            running: Arc::new(RwLock::new(false)),
            match_interval_secs,
            websocket_service: None,
            settlement: None,
            market_clearing: None,
            blockchain_service: None,
            grid_topology: GridTopologyService::new(),
        }
    }

    /// Set the Market Clearing service for processing escrow refunds
    pub fn with_market_clearing(mut self, market_clearing: MarketClearingService) -> Self {
        self.market_clearing = Some(market_clearing);
        self
    }

    /// Set the WebSocket service for broadcasting match events
    pub fn with_websocket(mut self, ws_service: WebSocketService) -> Self {
        self.websocket_service = Some(ws_service);
        self
    }

    /// Set the Settlement service for processing matched trades
    pub fn with_settlement(mut self, settlement: SettlementService) -> Self {
        self.settlement = Some(settlement);
        self
    }

    /// Set the Blockchain service for on-chain matching
    pub fn with_blockchain(mut self, blockchain_service: BlockchainService) -> Self {
        self.blockchain_service = Some(blockchain_service);
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

        info!(
            "🚀 Starting automated order matching engine (interval: {}s)",
            self.match_interval_secs
        );

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

    /// Minimum trade amount in kWh to avoid dust
    const MIN_TRADE_AMOUNT: Decimal = Decimal::from_parts(100000000, 0, 0, false, 9); // 0.100000000

    /// Expire orders that have passed their expiration time
    pub async fn expire_stale_orders(&self) -> Result<u64> {
        let now = chrono::Utc::now();
        
        // Fetch stale orders that need expiry
        let stale_orders_rows = sqlx::query(
            r#"
            SELECT 
                id, user_id, order_type, side, 
                energy_amount, price_per_kwh, filled_amount, status, 
                expires_at, created_at, filled_at, epoch_id, zone_id, meter_id, refund_tx_signature, order_pda,
                trigger_price, trigger_type, trigger_status, trailing_offset, session_token, triggered_at
            FROM trading_orders 
            WHERE status IN ('active', 'pending', 'partially_filled') 
            AND expires_at < $1
            "#,
        )
        .bind(now)
        .fetch_all(&self.db)
        .await?;

        let stale_orders: Vec<crate::models::trading::TradingOrderDb> = stale_orders_rows.into_iter().map(|row| {
             crate::models::trading::TradingOrderDb {
                id: row.get("id"),
                user_id: row.get("user_id"),
                order_type: row.get("order_type"),
                side: row.get("side"),
                energy_amount: row.get("energy_amount"),
                price_per_kwh: row.get("price_per_kwh"),
                filled_amount: row.get("filled_amount"),
                status: row.get("status"),
                expires_at: row.get("expires_at"),
                created_at: row.get("created_at"),
                filled_at: row.get("filled_at"),
                epoch_id: row.get("epoch_id"),
                zone_id: row.get("zone_id"),
                meter_id: row.get("meter_id"),
                refund_tx_signature: row.get("refund_tx_signature"),
                order_pda: row.get("order_pda"),
                session_token: row.get("session_token"),
                trigger_price: row.get("trigger_price"),
                trigger_type: row.get("trigger_type"),
                trigger_status: row.get("trigger_status"),
                trailing_offset: row.get("trailing_offset"),
                triggered_at: row.get("triggered_at"),
             }
        }).collect();

        let mut expired_count = 0;
        for order in stale_orders {
            info!("🕒 Expiring order {}: type={}, side={}, amount={}, status={}", 
                order.id, order.order_type.as_str(), order.side.as_str(), order.energy_amount, order.status.as_str());

            // 1. Update status to expired
            sqlx::query(
                "UPDATE trading_orders SET status = 'expired', updated_at = NOW() WHERE id = $1"
            )
            .bind(order.id)
            .execute(&self.db)
            .await?;

            // 2. Process Refund/Unlock
            if let Some(market_clearing) = &self.market_clearing {
                let remaining_amount = order.energy_amount - order.filled_amount.unwrap_or(Decimal::ZERO);
                
                if remaining_amount > Decimal::ZERO {
                    match order.side {
                        OrderSide::Buy => {
                            let refund_value = remaining_amount * order.price_per_kwh;
                            // The provided snippet for `receiver_wallet_addr` and `receiver_wallet` is incomplete and refers to an undefined `db_user`.
                            // Assuming it was meant to be part of a larger, separate change or a placeholder, it's omitted to maintain syntactic correctness.
                            if let Err(e) = market_clearing.unlock_funds(order.user_id, order.id, refund_value, "Order Expired").await {
                                error!("Failed to refund funds for expired order {}: {}", order.id, e);
                            } else {
                                info!("💰 Refunded {} for expired buy order {}", refund_value, order.id);
                            }
                        }
                        OrderSide::Sell => {
                            if let Err(e) = market_clearing.unlock_energy(order.user_id, order.id, remaining_amount, "Order Expired").await {
                                error!("Failed to unlock energy for expired order {}: {}", order.id, e);
                            } else {
                                info!("⚡ Unlocked {} energy for expired sell order {}", remaining_amount, order.id);
                            }
                        }
                    }
                }
            }

            expired_count += 1;
        }

        if expired_count > 0 {
            info!("🧹 Expired {} stale orders totaling", expired_count);
        }
        
        Ok(expired_count)
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

            // Cleanup expired orders first
            if let Err(e) = self.expire_stale_orders().await {
                error!("❌ Error expiring stale orders: {}", e);
            }

            // Run one matching cycle
            match self.match_orders_cycle().await {
                Ok(matches) => {
                    if matches > 0 {
                        info!(
                            "✅ Matching cycle completed: {} new transactions created",
                            matches
                        );
                    } else {
                        debug!("Matching cycle completed: no new matches");
                    }
                }
                Err(e) => {
                    error!("❌ Error in matching cycle: {}", e);
                }
            }

            // Sleep before next cycle
            tokio::time::sleep(Duration::from_secs(self.match_interval_secs)).await;
        }

        info!("Order matching loop terminated");
    }

    /// Run one matching cycle
    async fn match_orders_cycle(&self) -> Result<usize> {
        use crate::models::trading::TradingOrderDb;

        // Get all pending buy orders
        let buy_orders_rows = sqlx::query(
            r#"
            SELECT 
                id, user_id, energy_amount, price_per_kwh, filled_amount,
                epoch_id, zone_id, order_type, side, status,
                expires_at, created_at, filled_at, meter_id,
                refund_tx_signature, order_pda, session_token,
                trigger_price, trigger_type, trigger_status,
                trailing_offset, triggered_at
            FROM trading_orders
            WHERE side = 'buy'::order_side AND status IN ('pending', 'active', 'partially_filled')
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        let buy_orders_db: Vec<TradingOrderDb> = buy_orders_rows.into_iter().map(|row| {
            TradingOrderDb {
                id: row.get("id"),
                user_id: row.get("user_id"),
                energy_amount: row.get("energy_amount"),
                price_per_kwh: row.get("price_per_kwh"),
                filled_amount: row.get("filled_amount"),
                epoch_id: row.get("epoch_id"),
                zone_id: row.get("zone_id"),
                order_type: row.get("order_type"),
                side: row.get("side"),
                status: row.get("status"),
                expires_at: row.get("expires_at"),
                created_at: row.get("created_at"),
                filled_at: row.get("filled_at"),
                meter_id: row.get("meter_id"),
                refund_tx_signature: row.get("refund_tx_signature"),
                order_pda: row.get("order_pda"),
                session_token: row.get("session_token"),
                trigger_price: row.get("trigger_price"),
                trigger_type: row.get("trigger_type"),
                trigger_status: row.get("trigger_status"),
                trailing_offset: row.get("trailing_offset"),
                triggered_at: row.get("triggered_at"),
            }
        }).collect();

        info!("Fetched {} buy orders", buy_orders_db.len());

        // Get all pending sell orders
        // We load them into a mutable vector to track fills during this cycle
        let sell_orders_rows = sqlx::query(
            r#"
            SELECT 
                id, user_id, energy_amount, price_per_kwh, filled_amount,
                epoch_id, zone_id, order_type, side, status,
                expires_at, created_at, filled_at, meter_id,
                refund_tx_signature, order_pda, session_token,
                trigger_price, trigger_type, trigger_status,
                trailing_offset, triggered_at
            FROM trading_orders
            WHERE side = 'sell'::order_side AND status IN ('pending', 'active', 'partially_filled')
            ORDER BY price_per_kwh ASC, created_at ASC
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        let mut sell_orders_db: Vec<TradingOrderDb> = sell_orders_rows.into_iter().map(|row| {
            TradingOrderDb {
                id: row.get("id"),
                user_id: row.get("user_id"),
                energy_amount: row.get("energy_amount"),
                price_per_kwh: row.get("price_per_kwh"),
                filled_amount: row.get("filled_amount"),
                epoch_id: row.get("epoch_id"),
                zone_id: row.get("zone_id"),
                order_type: row.get("order_type"),
                side: row.get("side"),
                status: row.get("status"),
                expires_at: row.get("expires_at"),
                created_at: row.get("created_at"),
                filled_at: row.get("filled_at"),
                meter_id: row.get("meter_id"),
                refund_tx_signature: row.get("refund_tx_signature"),
                order_pda: row.get("order_pda"),
                session_token: row.get("session_token"),
                trigger_price: row.get("trigger_price"),
                trigger_type: row.get("trigger_type"),
                trigger_status: row.get("trigger_status"),
                trailing_offset: row.get("trailing_offset"),
                triggered_at: row.get("triggered_at"),
            }
        }).collect();

        info!("Fetched {} sell orders", sell_orders_db.len());

        if buy_orders_db.is_empty() || sell_orders_db.is_empty() {
            return Ok(0);
        }

        let mut matches_created = 0;

        // Try to match each buy order
        for buy_order in &buy_orders_db {
            let mut buy_filled_amount = buy_order.filled_amount.unwrap_or(Decimal::ZERO);
            let buy_energy_amount = buy_order.energy_amount;
            
            // Calculate remaining amount needed
            let mut remaining_buy_amount = buy_energy_amount - buy_filled_amount;
            
            // Dust protection: If remaining amount is too small, mark as filled/cancelled to stop matching
            if remaining_buy_amount < Self::MIN_TRADE_AMOUNT {
                if remaining_buy_amount > Decimal::ZERO {
                    // Start a new logical block to avoid borrowing issues if we were scanning orders
                    // But here we are just deciding to skip/close this buy order
                    let _ = sqlx::query("UPDATE trading_orders SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
                        .bind(buy_order.id)
                        .execute(&self.db).await;
                    info!("Cancelled dust buy order {} (rem: {})", buy_order.id, remaining_buy_amount);
                }
                continue; 
            }

            // 1. Calculate Landed Cost for all available sellers relative to THIS buyer
            // 2. Filter eligible sellers
            // 3. Sort by Landed Cost ASC
            
            // We create a list of indices to sell_orders_db to avoid cloning the whole structs
            struct Candidate {
                index: usize,
                landed_cost: Decimal,
                match_price: Decimal, // The base price (sell price)
                wheeling_charge_per_kwh: Decimal,
                loss_factor: Decimal,
                loss_cost_per_kwh: Decimal,
            }

            let mut candidates: Vec<Candidate> = Vec::new();

            for (idx, sell_order) in sell_orders_db.iter().enumerate() {
                let sell_filled = sell_order.filled_amount.unwrap_or(Decimal::ZERO);
                let sell_energy = sell_order.energy_amount;
                let remaining_sell = sell_energy - sell_filled;
                
                if remaining_sell < Self::MIN_TRADE_AMOUNT {
                    continue; // Skip dust entries
                }

                // Calculate Costs
                // If zone_id is missing, we use None which results in higher default fees
                let wheeling_charge = self.grid_topology.calculate_wheeling_charge(sell_order.zone_id, buy_order.zone_id);
                let loss_factor = self.grid_topology.calculate_loss_factor(sell_order.zone_id, buy_order.zone_id);
                
                let sell_price = sell_order.price_per_kwh;
                let loss_cost_unit = sell_price * loss_factor;
                let landed_price = sell_price + wheeling_charge + loss_cost_unit;

                // Check compatibility
                if landed_price <= buy_order.price_per_kwh {
                    candidates.push(Candidate {
                        index: idx,
                        landed_cost: landed_price,
                        match_price: sell_price,
                        wheeling_charge_per_kwh: wheeling_charge,
                        loss_factor,
                        loss_cost_per_kwh: loss_cost_unit,
                    });
                }
            }

            // Sort by Landed Cost ASC
            candidates.sort_by(|a, b| a.landed_cost.cmp(&b.landed_cost));

            // Execute matches against candidates
            for candidate in candidates {
                if remaining_buy_amount <= Decimal::ZERO {
                    break;
                }

                // Access the mutable sell order via index
                let sell_order = &mut sell_orders_db[candidate.index];
                
                let sell_filled = sell_order.filled_amount.unwrap_or(Decimal::ZERO);
                let remaining_sell = sell_order.energy_amount - sell_filled;

                if remaining_sell <= Decimal::ZERO {
                    continue;
                }

                // Match amount
                let match_amount = if remaining_buy_amount < remaining_sell {
                    remaining_buy_amount
                } else {
                    remaining_sell
                };

                let total_energy_cost = match_amount * candidate.match_price;
                let total_wheeling = match_amount * candidate.wheeling_charge_per_kwh;
                let total_loss_cost = match_amount * candidate.loss_cost_per_kwh;

                info!(
                    "Matching buy order {} with sell order {}: {} kWh at ${}/kWh base (Landed: ${})",
                    buy_order.id, sell_order.id, match_amount, candidate.match_price, candidate.landed_cost
                );

                let epoch_id = buy_order.epoch_id.or(sell_order.epoch_id)
                    .ok_or_else(|| anyhow::anyhow!("Epoch ID required"))?;

                // DB Actions
                match self.create_order_match(
                    epoch_id,
                    buy_order.id,
                    sell_order.id,
                    buy_order.user_id,
                    sell_order.user_id,
                    match_amount,
                    candidate.match_price,
                    total_energy_cost,
                    buy_order.order_pda.as_deref(),
                    sell_order.order_pda.as_deref(),
                ).await {
                    Ok(match_id) => {
                         matches_created += 1;
                         // metrics...
                         track_order_matched("p2p", match_amount.to_f64().unwrap_or(0.0));
                         track_trading_operation("match", true);

                         // Trigger settlement
                         // Note: We need to pass the extra costs to settlement service eventually.
                         // For now, we use the standard method.
                         self.trigger_settlement(
                            match_id, buy_order.id, sell_order.id, 
                            buy_order.user_id, sell_order.user_id, 
                            match_amount, candidate.match_price, total_energy_cost, epoch_id,
                            (total_wheeling, candidate.loss_factor, total_loss_cost, buy_order.zone_id, sell_order.zone_id),
                            buy_order.session_token.clone(), sell_order.session_token.clone()
                         ).await;

                         // Update In-Memory State
                         sell_order.filled_amount = Some(sell_filled + match_amount);
                         buy_filled_amount += match_amount;
                         remaining_buy_amount -= match_amount;

                         // Update DB - Sell Order
                         let new_sell_status = if sell_order.filled_amount.unwrap_or_default() >= sell_order.energy_amount {
                             OrderStatus::Filled
                         } else {
                             OrderStatus::PartiallyFilled
                         };
                         
                         let _ = sqlx::query("UPDATE trading_orders SET filled_amount = $1, status = $2, updated_at = NOW() WHERE id = $3")
                            .bind(sell_order.filled_amount)
                            .bind(new_sell_status)
                            .bind(sell_order.id)
                            .execute(&self.db).await;
                    },
                    Err(e) => {
                        error!("Failed to create match: {}", e);
                    }
                }
            }

            // Update DB - Buy Order (after processing all candidates)
            let new_buy_status = if buy_filled_amount >= buy_energy_amount {
                OrderStatus::Filled
            } else if buy_filled_amount > Decimal::ZERO {
                OrderStatus::PartiallyFilled
            } else {
                OrderStatus::Active
            };

            let _ = sqlx::query("UPDATE trading_orders SET filled_amount = $1, status = $2, updated_at = NOW() WHERE id = $3")
                .bind(buy_filled_amount)
                .bind(new_buy_status)
                .bind(buy_order.id)
                .execute(&self.db).await;
        }

        Ok(matches_created)
    }

    /// Create an order match record
    async fn create_order_match(
        &self,
        epoch_id: Uuid,
        buy_order_id: Uuid,
        sell_order_id: Uuid,
        _buyer_id: Uuid,
        _seller_id: Uuid,
        energy_amount: Decimal,
        price_per_kwh: Decimal,
        _total_price: Decimal,
        buy_order_pda: Option<&str>,
        sell_order_pda: Option<&str>,
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
            ) VALUES ($1, $2, $3, $4, $5, $6, NOW(), $7, NOW(), NOW())
            "#,
        )
        .bind(match_id)
        .bind(epoch_id)
        .bind(buy_order_id)
        .bind(sell_order_id)
        .bind(&energy_amount)
        .bind(&price_per_kwh)
        .bind(OrderStatus::Pending)
        .execute(&self.db)
        .await?;


        // 2. Execute On-Chain Match (if blockchain service is available)
        if let Some(blockchain) = &self.blockchain_service {
             // We need the authority keypair to sign the match
             match blockchain.get_authority_keypair().await {
                 Ok(authority) => {
                     // We need the Market Pubkey. 
                     // Ideally this comes from config or DB. 
                     // For now, let's derive it or fetch from env since there is usually 1 market per deployment.
                     // Or we can get it from the order_pda logic if we knew it.
                     // Let's rely on the instructions module knowing the program ID and deriving the market PDA if it's deterministic (seeds=[b"market"]).
                     // But execute_match_orders takes string pubkeys.
                     
                     // Helper to derive market PDA
                     let market_pda = Pubkey::find_program_address(&[b"market"], &blockchain.trading_program_id().unwrap_or_default()).0;
                     
                                             
                     if let (Some(b_pda), Some(s_pda)) = (buy_order_pda, sell_order_pda) {
                         let match_u64 = (energy_amount * Decimal::from(1_000_000_000)).to_u64().unwrap_or(0);
                         
                         info!("Executing on-chain match: Buyer {}, Seller {}, Amount {}", b_pda, s_pda, match_u64);
                         
                         match blockchain.execute_match_orders(&authority, &market_pda.to_string(), b_pda, s_pda, match_u64).await {
                             Ok(sig) => {
                                 info!("✅ On-chain match successful: {}", sig);
                                 // Update order_matches with signature?
                                 // Schema might not have it yet.
                             },
                             Err(e) => {
                                 error!("❌ On-chain match failed: {}", e);
                                 // We continue, as off-chain settlement is primary for now.
                             }
                         }
                     } else {
                         warn!("Skipping on-chain match: Missing Order PDAs for {} or {}", buy_order_id, sell_order_id);
                     }
                 },
                 Err(e) => error!("Failed to get authority keypair for matching: {}", e),
             }
        }

        // Broadcast order matched event via WebSocket
        if let Some(ws_service) = &self.websocket_service {
            // energy_amount and price_per_kwh are already f64, no need to parse
            let energy_f64 = energy_amount.to_f64().unwrap_or(0.0);
            let price_f64 = price_per_kwh.to_f64().unwrap_or(0.0);

            tokio::spawn({
                let ws = ws_service.clone();
                let mid = match_id.to_string();
                let buy_id = buy_order_id.to_string();
                let sell_id = sell_order_id.to_string();
                async move {
                    ws.broadcast_order_matched(buy_id, sell_id, mid, energy_f64, price_f64)
                        .await;
                }
            });
        }

        Ok(match_id)
    }

    /// Create settlement for the matched trade
    async fn trigger_settlement(
        &self,
        match_id: Uuid,
        buy_order_id: Uuid,
        sell_order_id: Uuid,
        buyer_id: Uuid,
        seller_id: Uuid,
        matched_amount: Decimal,
        match_price: Decimal,
        total_price: Decimal,
        epoch_id: Uuid,
        matches_costs: (Decimal, Decimal, Decimal, Option<i32>, Option<i32>), // wheeling, loss_factor, loss_cost, b_zone, s_zone
        buyer_session_token: Option<String>,
        seller_session_token: Option<String>,
    ) {
        if let Some(settlement) = &self.settlement {
            let (wheeling_charge, loss_factor, loss_cost, buyer_zone_id, seller_zone_id) = matches_costs;
            
            // Create a TradeMatch object to pass to settlement service
            let trade_match = TradeMatch {
                id: Uuid::new_v4(),
                match_id,
                buy_order_id,
                sell_order_id,
                buyer_id,
                seller_id,
                quantity: matched_amount,
                price: match_price,
                total_value: total_price,
                wheeling_charge,
                loss_factor,
                loss_cost,
                buyer_zone_id,
                seller_zone_id,
                matched_at: chrono::Utc::now(),
                epoch_id,
                buyer_session_token,
                seller_session_token,
            };

            // We create a temporary vector with one trade to reuse the existing method
            // Or better, expose create_settlement on settlement (it is public)
            match settlement.create_settlement(&trade_match).await {
                Ok(settlement) => {
                    info!(
                        "✅ Created settlement {} from match {}",
                        settlement.id, match_id
                    );

                    // Broadcast trade executed event via WebSocket
                    if let Some(ws_service) = &self.websocket_service {
                        let ws = ws_service.clone();
                        let s_id = settlement.id.to_string();
                        let b_order_id = buy_order_id.to_string();
                        let s_order_id = sell_order_id.to_string();
                        let b_id = buyer_id.to_string();
                        let s_id_user = seller_id.to_string();
                        let qty = matched_amount.to_string();
                        let prc = match_price.to_string();
                        let total = total_price.to_string();
                        let now = chrono::Utc::now().to_rfc3339();

                        tokio::spawn(async move {
                            ws.broadcast_trade_executed(
                                s_id, b_order_id, s_order_id, b_id, s_id_user, qty, prc, total, now,
                            )
                            .await;
                        });
                    }

                    // Update order_match with settlement_id
                    let _ =
                        sqlx::query("UPDATE order_matches SET settlement_id = $1 WHERE id = $2")
                            .bind(settlement.id)
                            .bind(match_id)
                            .execute(&self.db)
                            .await
                            .map_err(|e| error!("Failed to link settlement to match: {}", e));
                }
                Err(e) => error!(
                    "❌ Failed to create settlement for match {}: {}",
                    match_id, e
                ),
            }
        }
    }

    /// Manually trigger a matching cycle (for testing or API endpoints)
    pub async fn trigger_matching(&self) -> Result<usize> {
        info!("Manual matching trigger requested");
        self.match_orders_cycle().await
    }
}
