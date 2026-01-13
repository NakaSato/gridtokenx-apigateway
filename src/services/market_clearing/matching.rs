use anyhow::Result;
use chrono::Utc;
use rust_decimal::prelude::{ToPrimitive, FromPrimitive};
use rust_decimal::Decimal;

use sqlx::Row;
use uuid::Uuid;
use std::str::FromStr;
use tracing::{error, info};
use reqwest::Client;

use crate::database::schema::types::OrderStatus;
use crate::error::ApiError;
use crate::handlers::websocket::broadcaster::broadcast_p2p_order_update;
use super::MarketClearingService;
use super::types::{OrderMatch, Settlement};
use crate::middleware::metrics;

impl MarketClearingService {
    /// Run order matching algorithm for an epoch
    pub async fn run_order_matching(&self, epoch_id: Uuid) -> Result<Vec<OrderMatch>> {
        let start_time = std::time::Instant::now();
        info!("Starting order matching for epoch: {}", epoch_id);

        // Get current order book
        let (mut buy_orders, mut sell_orders) = self.get_order_book(epoch_id).await?;

        if buy_orders.is_empty() || sell_orders.is_empty() {
            info!("No orders to match in epoch: {}", epoch_id);
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let mut total_volume = Decimal::ZERO;
        let mut total_match_count = 0;

        // Order matching algorithm: price-time priority
        while let Some(buy_order) = buy_orders.first_mut() {
            if let Some(sell_order) = sell_orders.first_mut() {
                // Check if orders can be matched (bid >= ask)
                if buy_order.price_per_kwh >= sell_order.price_per_kwh {
                    // Calculate clearing price as midpoint of bid-ask spread
                    // This ensures fair pricing for both parties
                    let match_price = (buy_order.price_per_kwh + sell_order.price_per_kwh) 
                        / Decimal::from(2);

                    // Calculate match amount (minimum of remaining amounts)
                    let match_amount = buy_order
                        .energy_amount
                        .clone()
                        .min(sell_order.energy_amount.clone());

                    if match_amount > Decimal::ZERO {
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
                        matches.push(order_match.clone());

                        info!(
                            "🤝 MATCHED: BuyOrder({}) vs SellOrder({}) | Amount: {} kWh | Price: {} GRIDX | MatchID: {}",
                            order_match.buy_order_id,
                            order_match.sell_order_id,
                            order_match.matched_amount,
                            order_match.match_price,
                            order_match.id
                        );

                        // Update order amounts
                        buy_order.energy_amount -= match_amount_clone.clone();
                        sell_order.energy_amount -= match_amount_clone.clone();

                        // Update totals
                        total_volume += match_amount_clone.clone();
                        total_match_count += 1;

                        // Remove fully filled orders
                        info!(
                            "Buy order {} remaining amount: {}",
                            buy_order.order_id, buy_order.energy_amount
                        );
                        if buy_order.energy_amount <= Decimal::ZERO {
                            info!(
                                "Buy order {} is fully filled, updating status",
                                buy_order.order_id
                            );
                            self.update_order_status(buy_order.order_id, OrderStatus::Filled)
                                .await?;
                            
                            // Broadcast fully filled status
                            let _ = broadcast_p2p_order_update(
                                buy_order.order_id,
                                buy_order.user_id,
                                "buy".to_string(),
                                "filled".to_string(),
                                buy_order.original_amount.to_string(),
                                buy_order.original_amount.to_string(),
                                "0".to_string(),
                                buy_order.price_per_kwh.to_string(),
                            ).await;
                            
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
                            
                            // Broadcast partial fill status
                            let filled = buy_order.original_amount - buy_order.energy_amount;
                            let _ = broadcast_p2p_order_update(
                                buy_order.order_id,
                                buy_order.user_id,
                                "buy".to_string(),
                                "partially_filled".to_string(),
                                buy_order.original_amount.to_string(),
                                filled.to_string(),
                                buy_order.energy_amount.to_string(),
                                buy_order.price_per_kwh.to_string(),
                            ).await;
                        }

                        info!(
                            "Sell order {} remaining amount: {}",
                            sell_order.order_id, sell_order.energy_amount
                        );
                        if sell_order.energy_amount <= Decimal::ZERO {
                            info!(
                                "Sell order {} is fully filled, updating status",
                                sell_order.order_id
                            );
                            self.update_order_status(sell_order.order_id, OrderStatus::Filled)
                                .await?;
                            
                            // Broadcast fully filled status
                            let _ = broadcast_p2p_order_update(
                                sell_order.order_id,
                                sell_order.user_id,
                                "sell".to_string(),
                                "filled".to_string(),
                                sell_order.original_amount.to_string(),
                                sell_order.original_amount.to_string(),
                                "0".to_string(),
                                sell_order.price_per_kwh.to_string(),
                            ).await;
                            
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
                            
                            // Broadcast partial fill status
                            let filled = sell_order.original_amount - sell_order.energy_amount;
                            let _ = broadcast_p2p_order_update(
                                sell_order.order_id,
                                sell_order.user_id,
                                "sell".to_string(),
                                "partially_filled".to_string(),
                                sell_order.original_amount.to_string(),
                                filled.to_string(),
                                sell_order.energy_amount.to_string(),
                                sell_order.price_per_kwh.to_string(),
                            ).await;
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
            let total_match_value: Decimal = matches
                .iter()
                .map(|m| m.matched_amount * m.match_price)
                .fold(Decimal::ZERO, |acc, val| acc + val);
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
            match self.create_settlement(order_match).await {
                Ok(settlement) => {
                    // Broadcast trade executed event
                    self.websocket_service.broadcast_trade_executed(
                        settlement.id.to_string(),
                        order_match.buy_order_id.to_string(),
                        order_match.sell_order_id.to_string(),
                        settlement.buyer_id.to_string(),
                        settlement.seller_id.to_string(),
                        settlement.energy_amount.to_string(),
                        settlement.price_per_kwh.to_string(),
                        settlement.total_amount.to_string(),
                        Utc::now().to_rfc3339(),
                    ).await;
                },
                Err(e) => {
                    error!(
                        "Failed to create settlement for match {}: {}",
                        order_match.id, e
                    );
                }
            }
        }

        let clearing_duration = start_time.elapsed();
        metrics::track_market_clearing(clearing_duration.as_millis() as f64, true);
        metrics::track_trade_match(total_volume.to_f64().unwrap_or(0.0), matches.len() as u64);

        info!(
            "🏆 MATCHING COMPLETE [Epoch {}]: matched_count={}, total_volume={} kWh, clearing_price={} GRIDX",
            epoch_id,
            matches.len(),
            total_volume,
            matches.first().map(|m| m.match_price).unwrap_or(Decimal::ZERO)
        );

        Ok(matches)
    }

    /// Save order match to database
    pub(super) async fn save_order_match(&self, order_match: &OrderMatch) -> Result<()> {
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

    /// Create settlement for an order match
    pub(super) async fn create_settlement(&self, order_match: &OrderMatch) -> Result<Settlement> {
        // Get buyer and seller information from orders
        let buy_order = sqlx::query(
            "SELECT user_id, zone_id, session_token FROM trading_orders WHERE id = $1",
        )
        .bind(order_match.buy_order_id)
        .fetch_one(&self.db)
        .await?;

        let sell_order = sqlx::query(
            "SELECT user_id, zone_id, meter_id, session_token FROM trading_orders WHERE id = $1",
        )
        .bind(order_match.sell_order_id)
        .fetch_one(&self.db)
        .await?;

        // --- Zone Cost Calculation ---
        let mut wheeling_charge = Decimal::ZERO;
        let mut loss_factor = Decimal::ZERO;
        let mut loss_cost = Decimal::ZERO;
        let mut effective_energy = order_match.matched_amount;

        if let (Some(b_zone), Some(s_zone)) = (buy_order.get::<Option<i32>, _>("zone_id"), sell_order.get::<Option<i32>, _>("zone_id")) {
            info!("Calculating P2P costs between zones {} and {}", b_zone, s_zone);
            
            let simulator_url = std::env::var("SIMULATOR_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
            let client = Client::new();
            
            let calc_request = serde_json::json!({
                "buyer_zone_id": b_zone,
                "seller_zone_id": s_zone,
                "energy_amount": order_match.matched_amount.to_f64().unwrap_or(0.0),
                "agreed_price": order_match.match_price.to_f64().unwrap_or(0.0)
            });

            match client.post(&format!("{}/api/v1/p2p/calculate-cost", simulator_url))
                .json(&calc_request)
                .send()
                .await 
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(cost_data) = resp.json::<serde_json::Value>().await {
                        wheeling_charge = Decimal::from_f64(cost_data["wheeling_charge"].as_f64().unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
                        loss_factor = Decimal::from_f64(cost_data["loss_factor"].as_f64().unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
                        loss_cost = Decimal::from_f64(cost_data["loss_cost"].as_f64().unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
                        effective_energy = Decimal::from_f64(cost_data["effective_energy"].as_f64().unwrap_or(0.0)).unwrap_or(order_match.matched_amount);
                        info!("P2P Costs: wheeling={}, loss_factor={}, loss_cost={}, effective_energy={}", 
                            wheeling_charge, loss_factor, loss_cost, effective_energy);
                    }
                }
                _ => {
                    error!("Failed to fetch costs from simulator for zones {}->{}", s_zone, b_zone);
                    // Fallback to zero charges/losses if simulator fails
                }
            }
        }

        // Calculate settlement amounts
        let total_amount = order_match.matched_amount * order_match.match_price;
        let fee_rate = Decimal::from_str("0.01").expect("Invalid fee rate constant"); // 1% fee
        let fee_amount = total_amount * fee_rate;
        // Total settlement value includes fees and wheeling charges
        let net_amount = total_amount - fee_amount - wheeling_charge;

        // Fetch Seller Wallet for REC issuance (and potential future use)
        let seller_wallet_row = sqlx::query(
            "SELECT wallet_address FROM users WHERE id = $1",
        )
        .bind(sell_order.get::<Uuid, _>("user_id"))
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
        let seller_wallet_addr: Option<String> = seller_wallet_row.get("wallet_address");



        // =================================================================
        // NEW: Execute On-Chain Settlement (Escrow Release)
        // =================================================================
        // 1. Release Net Payment to Seller (Currency)
        // Note: Fees and Wheeling Charges remain in Authority Escrow (as revenue)
        // 4. Trigger Blockchain Settlement (Atomic Swap)
        // In a real implementation, this would build a single atomic transaction
        // For this demo, we'll do two transfers (USDC -> Seller, Energy -> Buyer)
        // NOTE: This is not truly atomic but sufficient for the MVP demo.

        // Transfer USDC from Escrow -> Seller
        match self
            .execute_escrow_release(
                sell_order.get("user_id"), 
                net_amount, 
                "currency"
            )
            .await
        {
            Ok(_sig) => info!("Settlement Payment Release triggered: {} -> Seller {}", net_amount, sell_order.get::<Uuid, _>("user_id")),
            Err(e) => error!("Failed to release payment escrow: {}", e),
        }

        // Transfer Energy from Escrow -> Buyer
        match self
            .execute_escrow_release(
                buy_order.get("user_id"), 
                effective_energy, 
                "energy"
            )
            .await
        {
            Ok(_sig) => info!("Settlement Energy Release triggered: {} -> Buyer {}", effective_energy, buy_order.get::<Uuid, _>("user_id")),
            Err(e) => error!("Failed to release energy escrow for {}: {}", buy_order.get::<Uuid, _>("user_id"), e),
        }


        let settlement = Settlement {
            id: Uuid::new_v4(),
            epoch_id: order_match.epoch_id,
            buyer_id: buy_order.get("user_id"),
            seller_id: sell_order.get("user_id"),
            energy_amount: order_match.matched_amount.clone(),
            price_per_kwh: order_match.match_price.clone(),
            total_amount: total_amount.clone(),
            fee_amount: fee_amount.clone(),
            wheeling_charge: wheeling_charge.clone(),
            loss_factor: loss_factor.clone(),
            loss_cost: loss_cost.clone(),
            effective_energy: effective_energy.clone(),
            buyer_zone_id: buy_order.get("zone_id"),
            seller_zone_id: sell_order.get("zone_id"),
            net_amount: net_amount.clone(),
            status: "pending".to_string(),
            buyer_session_token: buy_order.get("session_token"),
            seller_session_token: sell_order.get("session_token"),
        };

        // Save settlement
        sqlx::query(
            r#"
            INSERT INTO settlements (
                id, epoch_id, buyer_id, seller_id, energy_amount, 
                price_per_kwh, total_amount, fee_amount, wheeling_charge,
                loss_factor, loss_cost, effective_energy, buyer_zone_id,
                seller_zone_id, net_amount, status, buyer_session_token, seller_session_token
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            "#,
        )
        .bind(&settlement.id)
        .bind(&settlement.epoch_id)
        .bind(&settlement.buyer_id)
        .bind(&settlement.seller_id)
        .bind(&settlement.energy_amount)
        .bind(&settlement.price_per_kwh)
        .bind(&settlement.total_amount)
        .bind(&settlement.fee_amount)
        .bind(&settlement.wheeling_charge)
        .bind(&settlement.loss_factor)
        .bind(&settlement.loss_cost)
        .bind(&settlement.effective_energy)
        .bind(settlement.buyer_zone_id)
        .bind(settlement.seller_zone_id)
        .bind(&settlement.net_amount)
        .bind(&settlement.status)
        .bind(&settlement.buyer_session_token)
        .bind(&settlement.seller_session_token)
        .execute(&self.db)
        .await?;

        // Update order match with settlement ID
        sqlx::query(
            "UPDATE order_matches SET settlement_id = $1 WHERE id = $2",
        )
        .bind(settlement.id)
        .bind(order_match.id)
        .execute(&self.db)
        .await?;

        // =================================================================
        // NEW: Automated REC Issuance
        // =================================================================
        let sell_order_meter_id: Option<Uuid> = sell_order.get("meter_id");
        let sell_order_user_id: Uuid = sell_order.get("user_id");

        if let Some(m_id) = sell_order_meter_id {
            info!("🌿 Triggering automated REC issuance for settlement {} (Meter: {:?})", settlement.id, m_id);
            
            let erc_service = self.erc_service.clone();
            let seller_id = sell_order_user_id;
            let seller_wallet_str = seller_wallet_addr.clone().unwrap_or_default();
            let energy_amount = settlement.energy_amount;
            let settlement_id = settlement.id;
            
            // Fetch meter serial for REC metadata
            let meter_serial = sqlx::query("SELECT serial_number FROM meters WHERE id = $1")
                .bind(m_id)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<String, _>("serial_number"))
                .unwrap_or_else(|| format!("{:?}", m_id));

            tokio::spawn(async move {
                let cert_request = crate::services::erc::IssueErcRequest {
                    wallet_address: seller_wallet_str,
                    meter_id: Some(meter_serial),
                    kwh_amount: energy_amount,
                    expiry_date: Some(Utc::now() + chrono::Duration::days(365)), // 1 year expiry
                    metadata: Some(serde_json::json!({
                        "renewable_source": "Solar",
                        "validation_data": format!("Settlement: {}", settlement_id)
                    })),
                };

                // Use a system/platform authority as issuer for automated issuance
                // For now, using seller wallet as placeholder issuer if needed, 
                // but ErcService::issue_certificate takes an issuer_wallet string.
                let issuer_wallet = "PlatformAuthority"; 

                match erc_service.issue_certificate(seller_id, issuer_wallet, cert_request, Some(settlement_id)).await {
                    Ok(cert) => info!("✅ Automated REC issued: {} for settlement {}", cert.certificate_id, settlement_id),
                    Err(e) => error!("❌ Failed to issue automated REC: {}", e),
                }
            });
        }

        Ok(settlement)
    }
}
