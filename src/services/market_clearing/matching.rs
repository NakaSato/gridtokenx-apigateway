use anyhow::Result;
use chrono::Utc;
use rust_decimal::prelude::{ToPrimitive, FromPrimitive};
use rust_decimal::Decimal;

use sqlx::Row;
use uuid::Uuid;
use std::str::FromStr;
use tracing::{error, info, warn};
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

        // Order matching algorithm: Landed Cost priority
        // Instead of simple price-time matching, we find the best seller for each buyer
        // considering zonal wheeling charges and losses.
        while !buy_orders.is_empty() && !sell_orders.is_empty() {
            let buy_order = &buy_orders[0];
            let mut best_sell_idx = None;
            let mut max_surplus = Decimal::from(-1); // Initialize to indicate no match found
            let mut match_price = Decimal::ZERO;

            // Find the best seller for the current top buyer
            for (sell_idx, sell_order) in sell_orders.iter().enumerate() {
                // Estimate Zonal Costs for this pair
                let (wheeling, loss_factor) = self.estimate_zonal_costs(buy_order.zone_id, sell_order.zone_id).await.unwrap_or((Decimal::ZERO, Decimal::ZERO));
                
                // Landed Cost = Seller Ask + Wheeling Charge (per kWh) + (Loss Factor * Seller Ask)
                // Note: wheeling from estimate is for 1kWh
                let landed_cost = sell_order.price_per_kwh + wheeling + (loss_factor * sell_order.price_per_kwh);
                
                if buy_order.price_per_kwh >= landed_cost {
                    let surplus = buy_order.price_per_kwh - landed_cost;
                    if surplus > max_surplus {
                        max_surplus = surplus;
                        best_sell_idx = Some(sell_idx);
                        // Clearing price is midpoint of Bid and Landed Cost (for fairness)
                        match_price = (buy_order.price_per_kwh + landed_cost) / Decimal::from(2);
                    }
                }
            }

            if let Some(sell_idx) = best_sell_idx {
                let sell_order = &mut sell_orders[sell_idx];
                let buy_order = &mut buy_orders[0];

                // Calculate match amount (minimum of remaining amounts)
                let match_amount = buy_order.energy_amount.min(sell_order.energy_amount);

                if match_amount > Decimal::ZERO {
                    let match_amount_clone = match_amount;
                    let match_price_clone = match_price;

                    // Create order match
                    let order_match = OrderMatch {
                        id: Uuid::new_v4(),
                        epoch_id,
                        buy_order_id: buy_order.order_id,
                        sell_order_id: sell_order.order_id,
                        matched_amount: match_amount_clone,
                        match_price: match_price_clone,
                        match_time: Utc::now(),
                        status: "pending".to_string(),
                    };

                    // Save match to database
                    self.save_order_match(&order_match).await?;
                    matches.push(order_match.clone());

                    info!(
                        "🤝 LANDED COST MATCH: BuyOrder({}) vs SellOrder({}) | Amount: {} kWh | Price: {} GRIDX | Surplus: {} | MatchID: {}",
                        order_match.buy_order_id,
                        order_match.sell_order_id,
                        order_match.matched_amount,
                        order_match.match_price,
                        max_surplus,
                        order_match.id
                    );

                    // Update order amounts
                    buy_order.energy_amount -= match_amount_clone;
                    sell_order.energy_amount -= match_amount_clone;

                    // Update totals
                    total_volume += match_amount_clone;
                    total_match_count += 1;

                    // Remove fully filled/partially filled status logic (inline)
                    let b_id = buy_order.order_id;
                    let b_user = buy_order.user_id;
                    let b_orig = buy_order.original_amount;
                    let b_rem = buy_order.energy_amount;
                    let b_price = buy_order.price_per_kwh;

                    if b_rem <= Decimal::ZERO {
                        self.update_order_status(b_id, OrderStatus::Filled).await?;
                        let _ = broadcast_p2p_order_update(b_id, b_user, "buy".to_string(), "filled".to_string(), b_orig.to_string(), b_orig.to_string(), "0".to_string(), b_price.to_string()).await;
                        buy_orders.remove(0);
                    } else {
                        self.update_order_filled_amount(b_id, match_amount_clone).await?;
                        let filled = b_orig - b_rem;
                        let _ = broadcast_p2p_order_update(b_id, b_user, "buy".to_string(), "partially_filled".to_string(), b_orig.to_string(), filled.to_string(), b_rem.to_string(), b_price.to_string()).await;
                    }

                    // For the seller, we need to be careful with indices since we used an index from loop
                    let s_id = sell_order.order_id;
                    let s_user = sell_order.user_id;
                    let s_orig = sell_order.original_amount;
                    let s_rem = sell_order.energy_amount;
                    let s_price = sell_order.price_per_kwh;

                    if s_rem <= Decimal::ZERO {
                        self.update_order_status(s_id, OrderStatus::Filled).await?;
                        let _ = broadcast_p2p_order_update(s_id, s_user, "sell".to_string(), "filled".to_string(), s_orig.to_string(), s_orig.to_string(), "0".to_string(), s_price.to_string()).await;
                        sell_orders.remove(sell_idx);
                    } else {
                        self.update_order_filled_amount(s_id, match_amount_clone).await?;
                        let filled = s_orig - s_rem;
                        let _ = broadcast_p2p_order_update(s_id, s_user, "sell".to_string(), "partially_filled".to_string(), s_orig.to_string(), filled.to_string(), s_rem.to_string(), s_price.to_string()).await;
                    }
                }
            } else {
                // No matches possible for the top buyer anymore
                buy_orders.remove(0);
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
        // NEW: Execute Atomic On-Chain Settlement
        // =================================================================
        let buy_order_pda: Option<String> = buy_order.get("order_pda");
        let sell_order_pda: Option<String> = sell_order.get("order_pda");

        if let (Some(b_pda), Some(s_pda)) = (buy_order_pda, sell_order_pda) {
            info!("🚀 Triggering TRUE ATOMIC SWAP for Match {}", order_match.id);
            match self.execute_atomic_swap(
                buy_order.get("user_id"),
                sell_order.get("user_id"),
                &b_pda,
                &s_pda,
                order_match.matched_amount.clone(),
                order_match.match_price.clone(),
                wheeling_charge.clone(),
                fee_amount.clone(),
            ).await {
                Ok(sig) => info!("✅ Atomic Settlement successful: {}", sig),
                Err(e) => error!("❌ Atomic Settlement failed: {}", e),
            }
        } else {
            warn!("⚠️ Missing order PDAs for Match {}, falling back to legacy settlement", order_match.id);
            // Fallback (legacy)
            match self.execute_escrow_release(sell_order.get("user_id"), net_amount, "currency").await {
                Ok(_) => info!("Settlement Payment Release triggered"),
                Err(e) => error!("Failed payment release: {}", e),
            }
            match self.execute_escrow_release(buy_order.get("user_id"), effective_energy, "energy").await {
                Ok(_) => info!("Settlement Energy Release triggered"),
                Err(e) => error!("Failed energy release: {}", e),
            }
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

    /// Estimate zonal costs for matching selection
    async fn estimate_zonal_costs(&self, buyer_zone: Option<i32>, seller_zone: Option<i32>) -> Result<(Decimal, Decimal)> {
        if buyer_zone.is_none() || seller_zone.is_none() {
             return Ok((Decimal::ZERO, Decimal::ZERO));
        }
        let b_zone = buyer_zone.unwrap();
        let s_zone = seller_zone.unwrap();
        
        if b_zone == s_zone {
             // Intra-zone still has small losses and fees in this simulator (0.1 THB)
             // But we can check if it's Zero if desired. 
             // Simulator returns consistent values for b==s.
        }

        let simulator_url = std::env::var("SIMULATOR_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
        let client = Client::new();
        
        let calc_request = serde_json::json!({
            "buyer_zone_id": b_zone,
            "seller_zone_id": s_zone,
            "energy_amount": 1.0,
            "agreed_price": 1.0 
        });

        match client.post(&format!("{}/api/v1/p2p/calculate-cost", simulator_url))
            .json(&calc_request)
            .send()
            .await 
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(cost_data) = resp.json::<serde_json::Value>().await {
                    let wheeling = Decimal::from_f64(cost_data["wheeling_charge"].as_f64().unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
                    let loss_factor = Decimal::from_f64(cost_data["loss_factor"].as_f64().unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
                    return Ok((wheeling, loss_factor));
                }
            }
            _ => {
                error!("Failed to fetch costs from simulator for zones {}->{}", s_zone, b_zone);
            }
        }
        Ok((Decimal::ZERO, Decimal::ZERO))
    }
}
