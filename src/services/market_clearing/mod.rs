pub mod types;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use sqlx::PgPool;
use std::str::FromStr;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::schema::types::{EpochStatus, OrderSide, OrderStatus, OrderType};
use crate::error::ApiError;
use crate::services::BlockchainService;

pub use types::*;

use crate::config::Config;
use crate::services::{AuditLogger, WalletService};

#[derive(Clone, Debug)]
pub struct MarketClearingService {
    db: PgPool,
    blockchain_service: BlockchainService,
    config: Config,
    wallet_service: WalletService,
    audit_logger: AuditLogger,
}

impl MarketClearingService {
    pub fn new(
        db: PgPool,
        blockchain_service: BlockchainService,
        config: Config,
        wallet_service: WalletService,
        audit_logger: AuditLogger,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            config,
            wallet_service,
            audit_logger,
        }
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
        let mut total_volume = Decimal::ZERO;
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
                        if sell_order.energy_amount <= Decimal::ZERO {
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
            if let Err(e) = self.create_settlement(order_match).await {
                error!(
                    "Failed to create settlement for match {}: {}",
                    order_match.id, e
                );
            }
        }

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
    async fn update_order_filled_amount(&self, order_id: Uuid, amount: Decimal) -> Result<()> {
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
        total_volume: Decimal,
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
        let total_amount = order_match.matched_amount * order_match.match_price;
        let fee_rate = Decimal::from_str("0.01").expect("Invalid fee rate constant"); // 1% fee
        let fee_amount = total_amount * fee_rate;
        let net_amount = total_amount - fee_amount;

        // =================================================================
        // NEW: Execute On-Chain Transfer (Settlement)
        // =================================================================

        // 1. Fetch Wallets
        let buyer_wallet_row = sqlx::query!(
            "SELECT wallet_address FROM users WHERE id = $1",
            buy_order.user_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
        let seller_wallet_row = sqlx::query!(
            "SELECT wallet_address FROM users WHERE id = $1",
            sell_order.user_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let buyer_wallet_addr: Option<String> = buyer_wallet_row.wallet_address;
        let seller_wallet_addr: Option<String> = seller_wallet_row.wallet_address;

        // DEBUG: Force type check
        // let _: i32 = buyer_wallet_addr;

        // Explicit check to avoid pattern match Unsized error
        if buyer_wallet_addr.is_some() && seller_wallet_addr.is_some() {
            let buyer_wallet_str = buyer_wallet_addr.unwrap();
            let seller_wallet_str = seller_wallet_addr.unwrap();
            // We wrap blockchain operations in a block to catch errors without failing the DB transaction
            // In a real system, this should be a robust distributed transaction or queue-based
            let blockchain_result = async {
                let authority_keypair = self
                    .blockchain_service
                    .get_authority_keypair()
                    .await
                    .map_err(|e| format!("Failed to load authority: {}", e))?;

                let token_mint_str = std::env::var("ENERGY_TOKEN_MINT")
                    .unwrap_or_else(|_| "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur".to_string());
                let token_mint = Pubkey::from_str(&token_mint_str)
                    .map_err(|e| format!("Invalid mint: {}", e))?;

                let buyer_wallet = Pubkey::from_str(&buyer_wallet_str)
                    .map_err(|e| format!("Invalid buyer wallet: {}", e))?;
                let seller_wallet = Pubkey::from_str(&seller_wallet_str)
                    .map_err(|e| format!("Invalid seller wallet: {}", e))?;

                // 2. Ensure ATAs exist
                let buyer_ata = self
                    .blockchain_service
                    .ensure_token_account_exists(&authority_keypair, &buyer_wallet, &token_mint)
                    .await
                    .map_err(|e| format!("Failed to get buyer ATA: {}", e))?;
                let seller_ata = self
                    .blockchain_service
                    .ensure_token_account_exists(&authority_keypair, &seller_wallet, &token_mint)
                    .await
                    .map_err(|e| format!("Failed to get seller ATA: {}", e))?;

                // 3. Transfer Energy Tokens (Seller -> Buyer)
                // Assuming 9 decimals (1 GRX = 1 kWh = 1_000_000_000 units)
                let transfer_amount = (order_match.matched_amount * Decimal::from(1_000_000_000))
                    .to_u64()
                    .unwrap_or(0);

                if transfer_amount > 0 {
                    let signature = self
                        .blockchain_service
                        .transfer_tokens(
                            &authority_keypair,
                            &seller_ata, // From Seller
                            &buyer_ata,  // To Buyer
                            &token_mint,
                            transfer_amount,
                            9, // Decimals
                        )
                        .await
                        .map_err(|e| format!("Transfer failed: {}", e))?;

                    Ok::<String, String>(signature.to_string())
                } else {
                    Ok("Zero amount".to_string())
                }
            }
            .await;

            match blockchain_result {
                Ok(sig) => tracing::info!(
                    "Settlement on-chain transfer successful. Signature: {}",
                    sig
                ),
                Err(e) => tracing::error!("Settlement on-chain transfer failed: {}", e),
            }
        } else {
            tracing::warn!("Skipping on-chain settlement: Seller or Buyer missing wallet address");
        }

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

    /// Create a new trading order (DB and On-Chain)
    pub async fn create_order(
        &self,
        user_id: Uuid,
        side: OrderSide,
        order_type: OrderType,
        energy_amount: Decimal,
        price_per_kwh: Option<Decimal>,
        expiry_time: Option<DateTime<Utc>>,
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

        // 1. Insert into DB
        sqlx::query!(
            r#"
            INSERT INTO trading_orders (
                id, user_id, order_type, side, energy_amount, price_per_kwh,
                filled_amount, status, expires_at, created_at, epoch_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
            epoch.id
        )
        .execute(&self.db)
        .await?;

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

    async fn execute_on_chain_order_creation(
        &self,
        user_id: Uuid,
        order_id: Uuid,
        side: OrderSide,
        energy_amount: Decimal,
        price_per_kwh: Decimal,
    ) -> Result<()> {
        use base64::{engine::general_purpose, Engine as _};
        use solana_sdk::signature::{Keypair, Signer};

        // Fetch user keys
        let db_user = sqlx::query!(
            "SELECT wallet_address, encrypted_private_key, wallet_salt, encryption_iv FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let keypair = if let (Some(enc_key), Some(iv), Some(salt)) = (
            db_user.encrypted_private_key,
            db_user.encryption_iv,
            db_user.wallet_salt,
        ) {
            let master_secret = &self.config.encryption_secret;
            let enc_key_b64 = general_purpose::STANDARD.encode(enc_key);
            let iv_b64 = general_purpose::STANDARD.encode(iv);
            let salt_b64 = general_purpose::STANDARD.encode(salt);

            let private_key_bytes = WalletService::decrypt_private_key(
                master_secret,
                &enc_key_b64,
                &salt_b64,
                &iv_b64,
            )?;

            Keypair::from_base58_string(&bs58::encode(&private_key_bytes).into_string())
        } else {
            // Lazy wallet generation if missing
            info!("User {} missing keys, generating new wallet...", user_id);
            let master_secret = &self.config.encryption_secret;
            let new_keypair = Keypair::new();
            let pubkey = new_keypair.pubkey().to_string();

            let (enc_key_b64, salt_b64, iv_b64) =
                WalletService::encrypt_private_key(master_secret, &new_keypair.to_bytes())?;

            let enc_key_bytes = general_purpose::STANDARD.decode(&enc_key_b64)?;
            let salt_bytes = general_purpose::STANDARD.decode(&salt_b64)?;
            let iv_bytes = general_purpose::STANDARD.decode(&iv_b64)?;

            sqlx::query!(
                 "UPDATE users SET wallet_address=$1, encrypted_private_key=$2, wallet_salt=$3, encryption_iv=$4 WHERE id=$5",
                 pubkey, enc_key_bytes, salt_bytes, iv_bytes, user_id
            )
            .execute(&self.db)
            .await?;

            // Request Airdrop
            if let Err(e) = self.wallet_service.request_airdrop(&new_keypair.pubkey(), 2.0).await {
                error!("Airdrop failed for user {}: {}", user_id, e);
                // Continue for mock/sim if needed, but here we propagate if we want strictness.
                // In dev environment, this should succeed.
            }
            new_keypair
        };

        // On-chain tx
        let (signature, order_pda) = if self.config.tokenization.enable_real_blockchain {
            let trading_program_id = self.blockchain_service.trading_program_id()?;
            let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &trading_program_id);

            let multiplier = Decimal::from(1_000_000_000);
            let amount_u64 = (energy_amount * multiplier).to_u64().unwrap_or(0);
            let price_u64 = (price_per_kwh * multiplier).to_u64().unwrap_or(0);

            let (sig, pda) = self.blockchain_service.execute_create_order(
                &keypair,
                &market_pda.to_string(),
                amount_u64,
                price_u64,
                match side {
                    OrderSide::Buy => "buy",
                    OrderSide::Sell => "sell",
                },
                None,
            ).await?;
            (sig.to_string(), Some(pda))
        } else {
            (format!("mock_order_sig_{}", order_id), None)
        };

        // Update DB with signature and PDA
        if let Some(pda) = order_pda {
            sqlx::query(
                "UPDATE trading_orders SET blockchain_tx_signature = $1, order_pda = $2 WHERE id = $3",
            )
            .bind(&signature)
            .bind(pda.to_string())
            .bind(order_id)
            .execute(&self.db)
            .await?;
        } else {
            sqlx::query(
                "UPDATE trading_orders SET blockchain_tx_signature = $1 WHERE id = $2",
            )
            .bind(&signature)
            .bind(order_id)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }
}
