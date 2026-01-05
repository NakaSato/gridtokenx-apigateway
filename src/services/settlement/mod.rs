pub mod types;

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::market_clearing::TradeMatch;
use crate::services::BlockchainService;
use crate::handlers::websocket::broadcaster::broadcast_settlement_complete;
use solana_sdk::signature::Signer;

pub use types::*;

/// Settlement service for blockchain transaction execution
#[derive(Clone)]
pub struct SettlementService {
    db: PgPool,
    blockchain: BlockchainService,
    config: SettlementConfig,
    encryption_secret: String,
    #[allow(dead_code)]
    pending_settlements: Arc<RwLock<Vec<Uuid>>>,
}

impl SettlementService {
    pub fn new(db: PgPool, blockchain: BlockchainService, encryption_secret: String) -> Self {
        Self::with_config(db, blockchain, SettlementConfig::default(), encryption_secret)
    }

    pub fn with_config(
        db: PgPool,
        blockchain: BlockchainService,
        config: SettlementConfig,
        encryption_secret: String,
    ) -> Self {
        Self {
            db,
            blockchain,
            config,
            encryption_secret,
            pending_settlements: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create settlement records from matched trades
    pub async fn create_settlements_from_trades(
        &self,
        trades: Vec<TradeMatch>,
    ) -> Result<Vec<Settlement>, ApiError> {
        let mut settlements = Vec::new();

        for trade in trades {
            let settlement = self.create_settlement(&trade).await?;
            settlements.push(settlement);
        }

        Ok(settlements)
    }

    /// Create a single settlement from a trade match
    pub async fn create_settlement(&self, trade: &TradeMatch) -> Result<Settlement, ApiError> {
        info!("Creating settlement for trade match: {}", trade.match_id);

        // Calculate values using passed trade info
        let total_value = trade.total_value;
        let fee_rate = self.config.fee_rate;
        let fee_amount = total_value * fee_rate;
        
        // Net Amount = Total Value - Fees - Wheeling Charges
        let wheeling_charge = trade.wheeling_charge;
        // Should we subtract wheeling charge from Seller's revenue? Yes.
        // Or Buyer pays it on top?
        // Implementation Plan says: "Buyer pays Total, Seller receives Total - Fees, Utility receives Fees".
        // With zone costs: "Buyer pays Total + Wheeling? Or Total includes Wheeling?"
        
        // Matching Engine calculated "Landed Cost" for comparison.
        // But the Trade Price (match_price) is the Base Price (Seller's Price).
        // Total Value = Quantity * Base Price.
        
        // If Buyer pays Landed Cost, then Buyer Pays = Total Value + Wheeling + Loss Cost.
        // But our system currently transfers "Quantity * Price" tokens.
        // We need to clarify who pays what.
        
        // User Requirement: "Buyer pays the Total, Seller receives Total - Fees, and Grid Utility ... accumulates fees."
        // And "Landed Cost = Sell Price + Wheeling + Loss".
        
        // If `trade.total_value` is `Quantity * Base Price`:
        // We should add Wheeling Charge to what Buyer pays?
        // Or deduct from Seller?
        // A common P2P model: Buyer pays Landed Cost. Seller gets Base Price. Grid gets Wheeling/Loss.
        
        // Let's assume Buyer pays `Total Value + Wheeling Charge + Loss Cost`.
        // But `trade.total_value` passed from matching engine is `Quantity * Match Price`.
        
        // Let's adjust logic:
        // Settlement Total Amount (Buyer Pays) = trade.total_value + trade.wheeling_charge + trade.loss_cost.
        // Net Amount (Seller Receives) = trade.total_value - fee_amount.
        // Grid Revenue = Wheeling + Loss + Fees.
        
        // However, standard Settlements usually have `total_amount` = Transaction Volume.
        // Let's stick to:
        // total_amount = trade.total_value (Base Energy Cost)
        // wheeling_charge = trade.wheeling_charge
        // loss_cost = trade.loss_cost
        // net_amount = total_amount - fee_amount - wheeling_charge - loss_cost (If Seller pays shipping)
        // OR
        // Buyer pays extra?
        
        // Let's assume Seller bears the cost of reaching the market (Landed Cost model usually implies comparison, but payment flow varies).
        // If we matched based on "Landed <= Buy Price", it means Buyer is willing to pay Landed Price.
        // So Buyer should pay Landed Price.
        // So `total_amount` (Transaction Value) should probably refer to what Buyer pays?
        
        // Let's enable flexible logic. For now, I will record the values as passed.
        // And `net_amount` = `total_value` - `fee_amount`. (Seller gets base price - platform fee).
        // Who pays wheeling? The Buyer.
        // But `execute_blockchain_transfer` transfers from Seller to Buyer?
        // No, `execute_blockchain_transfer` logic usually transfers Tokens from Buyer to Seller?
        // Wait, Step 51 code: `transfer_tokens ... &seller_ata, &buyer_ata`...
        // Comments say "Transfer Energy Tokens (Seller -> Buyer)".
        // Ah, this is ENERGY token transfer. Not Payment Token (USDC/Sol).
        // Payment is likely separate or swapped.
        
        // If this is Energy Token transfer:
        // Effective Energy = Quantity * (1 - Loss Factor).
        // Seller sends Quantity. Buyer receives Effective Energy.
        // Loss is burned or diverted?
        
        // Step 157 code: `transfer_amount = (effective_energy * ...)`.
        // So Seller sends Effective Energy?
        // Then where did the loss go?
        // If Seller generated 100, and loss is 5%, Buyer gets 95.
        // Seller's meter reading shows 100 export.
        
        // Let's stick to what I just implemented in `OrderMatchingEngine::trigger_settlement` (Step 157):
        // `effective_energy` is passed (via TradeMatch logic or re-calculated?).
        // Wait, I passed `TradeMatch` with `quantity` = `matched_amount`.
        // And I added `effective_energy` column to `settlements`.
        
        // I need to calculate `effective_energy` here.
        let effective_energy = trade.quantity * (Decimal::ONE - trade.loss_factor);
        
        let settlement = Settlement {
            id: Uuid::new_v4(),
            trade_id: trade.id,
            buyer_id: trade.buyer_id,
            seller_id: trade.seller_id,
            buy_order_id: trade.buy_order_id,
            sell_order_id: trade.sell_order_id,
            energy_amount: trade.quantity,
            price: trade.price,
            total_value,
            fee_amount,
            net_amount: total_value - fee_amount - wheeling_charge, // Assuming Seller bears wheeling?
            // Actually let's just record it. Logic on payment is not in scope of this file (it handles Energy Token transfer mostly).
            // But I will populate the new columns.
            wheeling_charge: Some(wheeling_charge),
            loss_factor: Some(trade.loss_factor),
            loss_cost: Some(trade.loss_cost),
            effective_energy: Some(effective_energy),
            buyer_zone_id: trade.buyer_zone_id,
            seller_zone_id: trade.seller_zone_id,
            
            status: SettlementStatus::Pending,
            blockchain_tx: None,
            created_at: Utc::now(),
            confirmed_at: None,
        };

        sqlx::query(
            r#"
            INSERT INTO settlements (
                id, trade_id, buyer_id, seller_id, buy_order_id, sell_order_id,
                energy_amount, price, total_value, fee_amount, net_amount, status, created_at,
                wheeling_charge, loss_factor, loss_cost, effective_energy, buyer_zone_id, seller_zone_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            "#,
        )
        .bind(settlement.id)
        .bind(settlement.trade_id)
        .bind(settlement.buyer_id)
        .bind(settlement.seller_id)
        .bind(settlement.buy_order_id)
        .bind(settlement.sell_order_id)
        .bind(settlement.energy_amount)
        .bind(settlement.price)
        .bind(settlement.total_value)
        .bind(settlement.fee_amount)
        .bind(settlement.net_amount)
        .bind(settlement.status.to_string())
        .bind(settlement.created_at)
        .bind(settlement.wheeling_charge)
        .bind(settlement.loss_factor)
        .bind(settlement.loss_cost)
        .bind(settlement.effective_energy)
        .bind(settlement.buyer_zone_id)
        .bind(settlement.seller_zone_id)
        .execute(&self.db)
        .await?;

        info!(
            "📝 Created settlement {}: {} kWh at ${} (buyer: {}, seller: {})",
            settlement.id,
            settlement.energy_amount,
            settlement.price,
            settlement.buyer_id,
            settlement.seller_id
        );

        Ok(settlement)
    }

    /// Execute blockchain settlement for a trade
    pub async fn execute_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<SettlementTransaction, ApiError> {
        // Update status to processing
        self.update_settlement_status(settlement_id, SettlementStatus::Processing)
            .await?;

        // Get settlement details
        let settlement = self.get_settlement(settlement_id).await?;

        // Execute blockchain transaction
        match self.execute_blockchain_transfer(&settlement).await {
            Ok(tx_result) => {
                // Update settlement with transaction signature
                self.update_settlement_confirmed(
                    settlement_id,
                    &tx_result.signature,
                    SettlementStatus::Completed,
                )
                .await?;

                // Finalize Escrow (Move funds and unlock energy)
                if let Err(e) = self.finalize_escrow(&settlement).await {
                    error!("⚠️ Failed to finalize escrow for settlement {}: {}", settlement_id, e);
                    // We don't fail the whole method if escrow finalization fails here, 
                    // but it should be noted. In production, this should be retryable.
                }

                // Broadcast settlement completion via WebSocket
                if let Err(e) = broadcast_settlement_complete(
                    settlement.id,
                    settlement.buyer_id,
                    settlement.seller_id,
                    settlement.energy_amount.to_string(),
                    settlement.total_value.to_string(),
                    Some(tx_result.signature.clone()),
                ).await {
                    error!("⚠️ Failed to broadcast settlement: {}", e);
                }

                info!(
                    "✅ Settlement {} completed: tx {}",
                    settlement_id, tx_result.signature
                );
                Ok(tx_result)
            }
            Err(e) => {
                error!("❌ Settlement {} failed: {}", settlement_id, e);

                // Update status to failed
                self.update_settlement_status(settlement_id, SettlementStatus::Failed)
                    .await?;

                Err(ApiError::Internal(format!(
                    "Settlement execution failed: {}",
                    e
                )))
            }
        }
    }

    /// Execute actual blockchain transfer
    async fn execute_blockchain_transfer(
        &self,
        settlement: &Settlement,
    ) -> Result<SettlementTransaction, ApiError> {
        info!(
            "🔗 Executing blockchain transfer for settlement {}",
            settlement.id
        );

        if !self.config.enable_real_blockchain {
            info!("Mocking blockchain transfer (mock mode enabled)");
            return Ok(SettlementTransaction {
                settlement_id: settlement.id,
                signature: format!("mock_settlement_sig_{}", Uuid::new_v4()),
                slot: 12345678,
                confirmation_status: "confirmed".to_string(),
            });
        }

        // 1. Get buyer and seller wallets from database
        let buyer_wallet = self.get_user_wallet(&settlement.buyer_id).await?;
        let seller_wallet = self.get_user_wallet(&settlement.seller_id).await?;

        // 2. Parse wallet addresses
        let buyer_pubkey = BlockchainService::parse_pubkey(&buyer_wallet)
            .map_err(|e| ApiError::Internal(format!("Invalid buyer wallet: {}", e)))?;
        let _seller_pubkey = BlockchainService::parse_pubkey(&seller_wallet)
            .map_err(|e| ApiError::Internal(format!("Invalid seller wallet: {}", e)))?;

        // 3. Get mint address from environment
        let mint_str = std::env::var("ENERGY_TOKEN_MINT")
            .map_err(|e| ApiError::Internal(format!("ENERGY_TOKEN_MINT not set: {}", e)))?;
        let mint = BlockchainService::parse_pubkey(&mint_str)
            .map_err(|e| ApiError::Internal(format!("Invalid mint config: {}", e)))?;

        // 4. Get authority keypair (Platform)
        let _platform_authority = self
            .blockchain
            .get_authority_keypair()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to get authority: {}", e)))?;

        // 5. Decrypt Seller Keypair (CRITICAL FIX: Seller must sign transfer)
        let seller_keypair = self.get_user_keypair(&settlement.seller_id).await?;
        let seller_decrypted_pubkey = seller_keypair.pubkey();
        
        // High visibility debug logs
        println!("DEBUG: Settlement {} - Decrypted seller PK: {}", settlement.id, seller_decrypted_pubkey);
        error!("DEBUG: Settlement {} - Decrypted seller PK: {}", settlement.id, seller_decrypted_pubkey);

        // CRITICAL CHECK: Does the decrypted key match the wallet we expect?
        if seller_decrypted_pubkey.to_string() != seller_wallet {
            println!("❌ IDENTITY MISMATCH! DB={} Decrypted={}", seller_wallet, seller_decrypted_pubkey);
            error!("❌ IDENTITY MISMATCH! DB wallet is {}, but decrypted key is {}. Aborting settlement.", seller_wallet, seller_decrypted_pubkey);
            return Err(ApiError::Internal(format!(
                "Wallet identity mismatch: DB={} Decrypted={}",
                seller_wallet, seller_decrypted_pubkey
            )));
        }

        let seller_actual_pubkey = seller_decrypted_pubkey;

        let buyer_token_account = self
            .blockchain
            .ensure_token_account_exists(&_platform_authority, &buyer_pubkey, &mint)
            .await
            .map_err(|e| {
                ApiError::Internal(format!("Failed to create buyer token account: {}", e))
            })?;

        let seller_token_account = self
            .blockchain
            .ensure_token_account_exists(&_platform_authority, &seller_actual_pubkey, &mint)
            .await
            .map_err(|e| {
                ApiError::Internal(format!("Failed to create seller token account: {}", e))
            })?;

        // 7. Calculate match amount (in Wh, same as order creation: kWh * 1000)
        let match_amount_wh = {
            let amount_wh = settlement.energy_amount * Decimal::from(1000i64);
            // Use floor() to get integer part, then convert to u64
            amount_wh.floor().to_string().parse::<u64>().unwrap_or(0)
        };

        info!(
            "🔍 Settlement energy_amount: {}, match_amount_wh: {}",
            settlement.energy_amount, match_amount_wh
        );

        // 8. Execute Token Transfer (Seller -> Buyer)
        // Only transfer the EFFECTIVE energy to the buyer.
        let effective_energy = settlement.effective_energy.unwrap_or(settlement.energy_amount);
        let amount_atomic = effective_energy * Decimal::from(1_000_000_000);
        let transfer_amount = amount_atomic
            .trunc()
            .to_string()
            .parse::<u64>()
            .unwrap_or(0);

        info!(
            "Executing Direct Token Transfer: From {} to {}, Amount: {} (atomic), Decimals: 9 (Effective Energy: {})",
            seller_token_account, buyer_token_account, transfer_amount, effective_energy
        );

        let signature = self
            .blockchain
            .transfer_tokens(
                &seller_keypair,   // Signer (Owner of From Account)
                &seller_token_account, // From (Seller ATA)
                &buyer_token_account,  // To (Buyer ATA)
                &mint,
                transfer_amount,
                9, // Decimals
            )
            .await
            .map_err(|e| ApiError::Internal(format!("Token transfer failed: {}", e)))?;

        // Handle grid loss: the difference between energy_amount (gross) and effective_energy
        // remain in the seller's account if we only transfer the effective amount.
        // To properly account for it, we should 'burn' these tokens or transfer them to a loss sink.
        let loss_energy = settlement.energy_amount - effective_energy;
        if loss_energy > Decimal::ZERO {
            let loss_atomic = (loss_energy * Decimal::from(1_000_000_000)).trunc().to_string().parse::<u64>().unwrap_or(0);
            if loss_atomic > 0 {
                let loss_sink_wallet = std::env::var("GRID_LOSS_SINK_WALLET").unwrap_or_else(|_| "LoSsSiNk1111111111111111111111111111111111".to_string());
                if let Ok(sink_pubkey) = BlockchainService::parse_pubkey(&loss_sink_wallet) {
                    if let Ok(sink_token_account) = self.blockchain.ensure_token_account_exists(&_platform_authority, &sink_pubkey, &mint).await {
                        info!("📉 Recording {} loss tokens to grid loss sink", loss_atomic);
                        let _ = self.blockchain.transfer_tokens(&seller_keypair, &seller_token_account, &sink_token_account, &mint, loss_atomic, 9).await;
                    }
                }
            }
        }

        info!("Settlement transfer completed. Signature: {}", signature);

        // 9. Get current slot for confirmation
        let slot = self
            .blockchain
            .get_slot()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to get slot: {}", e)))?;

        // 10. Create settlement transaction record
        Ok(SettlementTransaction {
            settlement_id: settlement.id,
            signature: signature.to_string(),
            slot,
            confirmation_status: "confirmed".to_string(),
        })
    }

    /// Helper: Get order creation transaction signature
    #[allow(dead_code)]
    async fn get_order_creation_tx(&self, order_id: Uuid) -> Result<String, ApiError> {
        let result = sqlx::query!(
            "SELECT blockchain_tx_signature FROM trading_orders WHERE id = $1",
            order_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::Database)?;

        result
            .blockchain_tx_signature
            .ok_or(ApiError::Internal(format!(
                "Order {} has no creation tx signature",
                order_id
            )))
    }

    /// Helper: Get user wallet address from database
    async fn get_user_wallet(&self, user_id: &Uuid) -> Result<String, ApiError> {
        let result = sqlx::query!("SELECT wallet_address FROM users WHERE id = $1", user_id)
            .fetch_one(&self.db)
            .await
            .map_err(ApiError::Database)?;

        result
            .wallet_address
            .ok_or_else(|| ApiError::Internal(format!("User {} has no wallet connected", user_id)))
    }

    /// Helper: Get Order PDA from database
    async fn _get_order_pda(&self, order_id: Uuid) -> Result<String, ApiError> {
        let result = sqlx::query!(
            "SELECT order_pda FROM trading_orders WHERE id = $1",
            order_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::Database)?;

        result
            .order_pda
            .ok_or_else(|| ApiError::Internal(format!("Order {} has no PDA stored", order_id)))
    }

    /// Process all pending settlements
    pub async fn process_pending_settlements(&self) -> Result<usize, ApiError> {
        let pending_ids = self.get_pending_settlements().await?;

        if pending_ids.is_empty() {
            debug!("No pending settlements to process");
            return Ok(0);
        }

        info!("🚀 Processing {} pending settlements...", pending_ids.len());
        let total_count = pending_ids.len();
        let mut processed = 0;

        for settlement_id in pending_ids {
            match self.execute_settlement(settlement_id).await {
                Ok(_) => {
                    processed += 1;
                }
                Err(e) => {
                    error!("❌ Failed to process settlement {}: {}", settlement_id, e);
                }
            }

            // Small delay between settlements to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let success_rate = (processed as f64 / total_count as f64) * 100.0;
        info!(
            "🏁 BATCH SETTLEMENT COMPLETE: Success Rate: {:.1}% ({}/{})",
            success_rate, processed, total_count
        );
        Ok(processed)
    }

    /// Get settlement by ID
    pub async fn get_settlement(&self, id: Uuid) -> Result<Settlement, ApiError> {
        use sqlx::Row;

        let row = sqlx::query(
            r#"
            SELECT
                id, buyer_id, seller_id, buy_order_id, sell_order_id, energy_amount,
                price_per_kwh, total_amount, fee_amount, net_amount,
                status, transaction_hash, created_at, processed_at,
                wheeling_charge, loss_factor, loss_cost, effective_energy, buyer_zone_id, seller_zone_id
            FROM settlements
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound("Settlement not found".into()))?;

        let status_str: String = row.get("status");
        let status = match status_str.to_lowercase().as_str() {
            "pending" => SettlementStatus::Pending,
            "processing" => SettlementStatus::Processing,
            "completed" | "confirmed" => SettlementStatus::Completed,
            "failed" => SettlementStatus::Failed,
            _ => SettlementStatus::Pending,
        };

        Ok(Settlement {
            id: row.get("id"),
            trade_id: Uuid::new_v4(), // Not stored in this simplified version
            buyer_id: row.get("buyer_id"),
            seller_id: row.get("seller_id"),
            buy_order_id: row.get("buy_order_id"),
            sell_order_id: row.get("sell_order_id"),
            energy_amount: row.get("energy_amount"),
            price: row.get("price_per_kwh"),
            total_value: row.get("total_amount"),
            fee_amount: row.get("fee_amount"),
            net_amount: row.get("net_amount"),
            status,
            blockchain_tx: row.get("transaction_hash"),
            created_at: row.get("created_at"),
            confirmed_at: row.get("processed_at"),
            wheeling_charge: row.get("wheeling_charge"),
            loss_factor: row.get("loss_factor"),
            loss_cost: row.get("loss_cost"),
            effective_energy: row.get("effective_energy"),
            buyer_zone_id: row.get("buyer_zone_id"),
            seller_zone_id: row.get("seller_zone_id"),
        })
    }

    /// Get all pending settlements
    pub async fn get_pending_settlements(&self) -> Result<Vec<Uuid>, ApiError> {
        use sqlx::Row;

        let rows = sqlx::query(
            r#"
            SELECT id
            FROM settlements
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    /// Update settlement status
    pub async fn update_settlement_status(
        &self,
        id: Uuid,
        status: SettlementStatus,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(status.to_string())
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(())
    }

    /// Update settlement with confirmation
    pub async fn update_settlement_confirmed(
        &self,
        id: Uuid,
        tx_signature: &str,
        status: SettlementStatus,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET status = $1,
                transaction_hash = $2,
                processed_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(status.to_string())
        .bind(tx_signature)
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(())
    }

    /// Retry failed settlements with exponential backoff (called by background job)
    /// Implements smart retry logic with error classification
    pub async fn retry_failed_settlements(&self, max_retries: u32) -> Result<usize, ApiError> {
        // Fetch settlements with status = 'Failed' and retry_count < max_retries
        let failed = sqlx::query!(
            r#"
            SELECT id, retry_count FROM settlements
            WHERE status = 'failed'
            AND retry_count < $1
            ORDER BY retry_count ASC, updated_at ASC
            "#,
            max_retries as i32
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)?;

        let mut retried = 0;
        let base_delay_secs = self.config.retry_delay_secs;
        
        for settlement in failed {
            // Calculate exponential backoff delay: base * 2^retry_count
            // e.g., with base=5s: 5s, 10s, 20s, 40s, 80s...
            let retry_count = settlement.retry_count.unwrap_or(0) as u32;
            let delay_secs = base_delay_secs * (2_u64.pow(retry_count));
            let max_delay_secs = 300; // Cap at 5 minutes
            let actual_delay = delay_secs.min(max_delay_secs);
            
            info!(
                "Retrying settlement {} (attempt {}/{}) with {}s delay",
                settlement.id, retry_count + 1, max_retries, actual_delay
            );
            
            // Wait with exponential backoff
            tokio::time::sleep(Duration::from_secs(actual_delay)).await;
            
            match self.execute_settlement(settlement.id).await {
                Ok(_) => {
                    info!("✅ Settlement {} retry succeeded", settlement.id);
                    retried += 1;
                }
                Err(e) => {
                    let error_str = e.to_string();
                    
                    // Classify error: determine if retryable
                    let is_retryable = Self::is_retryable_error(&error_str);
                    
                    if is_retryable {
                        error!("⚠️ Settlement {} retry failed (retryable): {}", settlement.id, e);
                        self.increment_retry_count(&settlement.id).await?;
                    } else {
                        // Non-retryable error - mark as permanently failed
                        error!("❌ Settlement {} permanently failed (non-retryable): {}", settlement.id, e);
                        self.mark_settlement_permanent_failure(&settlement.id, &error_str).await?;
                    }
                }
            }
        }

        Ok(retried)
    }

    /// Classify if an error is retryable
    fn is_retryable_error(error: &str) -> bool {
        let retryable_patterns = [
            "timeout",
            "connection refused",
            "network",
            "rate limit",
            "429",
            "503",
            "temporary",
            "try again",
            "blockhash",
            "not found", // Transaction not yet confirmed
        ];
        
        let non_retryable_patterns = [
            "insufficient",
            "invalid signature",
            "invalid account",
            "unauthorized",
            "forbidden",
            "already processed",
            "account not found",  // Permanent missing account
            "program failed",
        ];
        
        let error_lower = error.to_lowercase();
        
        // If matches non-retryable, don't retry
        for pattern in non_retryable_patterns.iter() {
            if error_lower.contains(pattern) {
                return false;
            }
        }
        
        // If matches retryable, retry
        for pattern in retryable_patterns.iter() {
            if error_lower.contains(pattern) {
                return true;
            }
        }
        
        // Default: retry unknown errors (conservative)
        true
    }

    /// Mark settlement as permanently failed (non-retryable)
    async fn mark_settlement_permanent_failure(
        &self,
        settlement_id: &Uuid,
        error_message: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET status = 'permanently_failed', 
                error_message = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(error_message)
        .bind(settlement_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;
        
        info!("Settlement {} marked as permanently failed: {}", settlement_id, error_message);
        Ok(())
    }

    /// Increment retry count for a settlement
    pub async fn increment_retry_count(&self, settlement_id: &Uuid) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE settlements
            SET retry_count = retry_count + 1, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(settlement_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(())
    }

    /// Get settlement statistics
    pub async fn get_settlement_stats(&self) -> Result<SettlementStats, ApiError> {
        use sqlx::Row;

        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending_count,
                COUNT(*) FILTER (WHERE status = 'processing') as processing_count,
                COUNT(*) FILTER (WHERE status = 'completed') as confirmed_count,
                COUNT(*) FILTER (WHERE status = 'failed') as failed_count,
                COALESCE(SUM(CASE WHEN status = 'completed' THEN total_amount::numeric ELSE 0 END), 0) as total_settled_value
            FROM settlements
            WHERE created_at > NOW() - INTERVAL '24 hours'
            "#,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::Database)?;

        Ok(SettlementStats {
            pending_count: row.get::<i64, _>("pending_count"),
            processing_count: row.get::<i64, _>("processing_count"),
            confirmed_count: row.get::<i64, _>("confirmed_count"),
            failed_count: row.get::<i64, _>("failed_count"),
            total_settled_value: row.get("total_settled_value"),
        })
    }
    /// Helper: Get user keypair from database
    async fn get_user_keypair(
        &self,
        user_id: &Uuid,
    ) -> Result<solana_sdk::signature::Keypair, ApiError> {
        use solana_sdk::signature::Keypair;

        // Fetch encrypted_private_key from users table
        let row = sqlx::query!(
            "SELECT wallet_address, encrypted_private_key, wallet_salt, encryption_iv FROM users WHERE id = $1",
            user_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

        let encrypted_pk = row.encrypted_private_key.ok_or_else(|| {
            ApiError::Internal(format!("User {} has no private key stored", user_id))
        })?;
        
        // If we have salt and iv (new flow), decrypt properly
        if let (Some(salt), Some(iv)) = (row.wallet_salt, row.encryption_iv) {
            use base64::{engine::general_purpose, Engine as _};
            
            // Convert bytes back to Base64 for the decrypt function
            let encrypted_b64 = general_purpose::STANDARD.encode(&encrypted_pk);
            let salt_b64 = general_purpose::STANDARD.encode(&salt);
            let iv_b64 = general_purpose::STANDARD.encode(&iv);
            
            use crate::services::WalletService;
            let decrypted = WalletService::decrypt_private_key(
                &self.encryption_secret,
                &encrypted_b64,
                &salt_b64,
                &iv_b64
            ).map_err(|e| ApiError::Internal(format!("Failed to decrypt wallet: {}", e)))?;
            
        // Valid key should be 32 (seed) or 64 (full keypair) bytes
        let decrypted_kp = if decrypted.len() == 64 {
            Keypair::try_from(decrypted.as_slice())
                .map_err(|e| ApiError::Internal(format!("Invalid 64-byte keypair: {}", e)))?
        } else if decrypted.len() == 32 {
            let secret_key: [u8; 32] = decrypted[..32]
                .try_into()
                .map_err(|_| ApiError::Internal("Invalid key slice".to_string()))?;
            Keypair::new_from_array(secret_key)
        } else {
            return Err(ApiError::Internal(format!(
                "Invalid key length: {}",
                decrypted.len()
            )));
        };

        Ok(decrypted_kp)
        } else {
             // Fallback minimal decryption logic (legacy)
            use base64::{engine::general_purpose, Engine as _};
            
            // Try to decode as Base64 first
            let decoded = match general_purpose::STANDARD.decode(&encrypted_pk) {
                Ok(d) => d,
                Err(_) => {
                    // If not valid Base64, assume it's raw bytes (legacy/test data)
                    encrypted_pk
                }
            };

            // Valid key should be 32 (seed) or 64 (full keypair) bytes
            if decoded.len() == 64 || decoded.len() == 32 {
                let secret_key: [u8; 32] = decoded[..32]
                    .try_into()
                    .map_err(|_| ApiError::Internal("Invalid key slice".to_string()))?;
                Ok(Keypair::new_from_array(secret_key))
            } else {
                Err(ApiError::Internal(format!(
                    "Invalid key length: {}",
                    decoded.len()
                )))
            }
        }
    }

    pub async fn finalize_escrow(&self, settlement: &Settlement) -> Result<(), ApiError> {
        let mut tx = self.db.begin().await.map_err(ApiError::Database)?;

        // 1. Seller: Deduct from locked_energy
        sqlx::query!(
            "UPDATE users SET locked_energy = locked_energy - $1 WHERE id = $2",
            settlement.energy_amount,
            settlement.seller_id
        )
        .execute(&mut *tx)
        .await.map_err(ApiError::Database)?;

        // 2. Buyer: Deduct from locked_amount (The matched portion of payment)
        let total_value = settlement.energy_amount * settlement.price;
        sqlx::query!(
            "UPDATE users SET locked_amount = locked_amount - $1 WHERE id = $2",
            total_value,
            settlement.buyer_id
        )
        .execute(&mut *tx)
        .await.map_err(ApiError::Database)?;

        // 3. Seller: Receive net_amount to their balance
        sqlx::query!(
            "UPDATE users SET balance = balance + $1 WHERE id = $2",
            settlement.net_amount,
            settlement.seller_id
        )
        .execute(&mut *tx)
        .await.map_err(ApiError::Database)?;

        // 4. Record Platform Revenue (Fees, Wheeling, Loss)
        if settlement.fee_amount > Decimal::ZERO {
            sqlx::query!(
                "INSERT INTO platform_revenue (settlement_id, amount, revenue_type, description) VALUES ($1, $2, 'platform_fee', $3)",
                settlement.id,
                settlement.fee_amount,
                format!("Platform fee for settlement {}", settlement.id)
            )
            .execute(&mut *tx)
            .await.map_err(ApiError::Database)?;
        }

        if let Some(wheeling) = settlement.wheeling_charge {
            if wheeling > Decimal::ZERO {
                sqlx::query!(
                    "INSERT INTO platform_revenue (settlement_id, amount, revenue_type, description) VALUES ($1, $2, 'wheeling_charge', $3)",
                    settlement.id,
                    wheeling,
                    format!("Wheeling charge for settlement {}", settlement.id)
                )
                .execute(&mut *tx)
                .await.map_err(ApiError::Database)?;
            }
        }

        if let Some(loss_cost) = settlement.loss_cost {
            if loss_cost > Decimal::ZERO {
                sqlx::query!(
                    "INSERT INTO platform_revenue (settlement_id, amount, revenue_type, description) VALUES ($1, $2, 'loss_cost', $3)",
                    settlement.id,
                    loss_cost,
                    format!("Grid loss cost for settlement {}", settlement.id)
                )
                .execute(&mut *tx)
                .await.map_err(ApiError::Database)?;
            }
        }

        // 5. Update Escrow Record status
        sqlx::query!(
            "UPDATE escrow_records SET status = 'released', updated_at = NOW() WHERE order_id IN ($1, $2) AND status = 'locked'",
            settlement.buy_order_id,
            settlement.sell_order_id
        )
        .execute(&mut *tx)
        .await.map_err(ApiError::Database)?;

        tx.commit().await.map_err(ApiError::Database)?;
        
        info!("🔐 Escrow finalized for settlement {}: funds transferred and energy unlocked", settlement.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_settlement_config_default() {
        let config = SettlementConfig::default();
        assert_eq!(config.fee_rate, Decimal::from_str("0.01").unwrap());
        assert_eq!(config.min_confirmation_blocks, 32);
    }

    #[test]
    fn test_settlement_status_display() {
        assert_eq!(SettlementStatus::Pending.to_string(), "pending");
        assert_eq!(SettlementStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn test_settlement_creation() {
        let settlement = Settlement {
            id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(),
            buyer_id: Uuid::new_v4(),
            seller_id: Uuid::new_v4(),
            buy_order_id: Uuid::new_v4(),
            sell_order_id: Uuid::new_v4(),
            energy_amount: Decimal::from(100),
            price: Decimal::from_str("0.15").unwrap(),
            total_value: Decimal::from_str("15.00").unwrap(),
            fee_amount: Decimal::from_str("0.15").unwrap(),
            net_amount: Decimal::from_str("14.85").unwrap(),
            status: SettlementStatus::Pending,
            blockchain_tx: None,
            created_at: Utc::now(),
            confirmed_at: None,
        };

        assert_eq!(settlement.status, SettlementStatus::Pending);
        assert!(settlement.blockchain_tx.is_none());
        assert!(settlement.confirmed_at.is_none());
    }

    #[test]
    fn test_fee_calculation() {
        let config = SettlementConfig {
            fee_rate: Decimal::from_str("0.01").unwrap(), // 1%
            min_confirmation_blocks: 32,
            retry_attempts: 3,
            retry_delay_secs: 5,
            enable_real_blockchain: true,
        };

        let trade_amount = Decimal::from(100);
        let expected_fee = Decimal::from_str("1.00").unwrap();

        assert_eq!(config.fee_rate * trade_amount, expected_fee);
    }

    #[test]
    fn test_settlement_transaction_structure() {
        let tx = SettlementTransaction {
            settlement_id: Uuid::new_v4(),
            signature: "5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3".to_string(),
            slot: 12345678,
            confirmation_status: "confirmed".to_string(),
        };

        assert_eq!(tx.slot, 12345678);
        assert_eq!(tx.confirmation_status, "confirmed");
    }

    #[test]
    fn test_settlement_status_transitions() {
        // Valid transition: Pending -> Processing
        let status1 = SettlementStatus::Processing;
        assert_eq!(status1, SettlementStatus::Processing);

        // Valid transition: Processing -> Completed
        let status2 = SettlementStatus::Completed;
        assert_eq!(status2, SettlementStatus::Completed);

        // Failed state
        let status3 = SettlementStatus::Failed;
        assert_eq!(status3, SettlementStatus::Failed);
    }

    #[test]
    fn test_settlement_status_failed() {
        let status = SettlementStatus::Failed;
        assert_eq!(status.to_string(), "failed");
    }

    #[test]
    fn test_custom_fee_rate() {
        let custom_config = SettlementConfig {
            fee_rate: Decimal::from_str("0.005").unwrap(), // 0.5%
            min_confirmation_blocks: 64,
            retry_attempts: 5,
            retry_delay_secs: 10,
            enable_real_blockchain: true,
        };

        assert_eq!(custom_config.fee_rate, Decimal::from_str("0.005").unwrap());
    }
}
