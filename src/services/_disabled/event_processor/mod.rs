pub mod types;

use anyhow::Result;
use chrono::Utc;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::EventProcessorConfig;
use crate::services::webhook::WebhookService;

pub use types::*;

#[derive(Clone)]
pub struct EventProcessorService {
    rpc_client: Arc<RpcClient>,
    db: Arc<PgPool>,
    config: EventProcessorConfig,
    #[allow(dead_code)]
    energy_token_mint: String,
    // WebSocket client would go here
    // pubsub_client: Arc<PubsubClient>,
    retry_count: Arc<AtomicU64>,
    replay_status: Arc<Mutex<Option<ReplayStatus>>>,
    webhook_service: WebhookService,
}

impl EventProcessorService {
    /// Create new event processor service
    pub fn new(
        db: Arc<PgPool>,
        rpc_url: String,
        config: EventProcessorConfig,
        energy_token_mint: String,
    ) -> Self {
        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let webhook_service =
            WebhookService::new(config.webhook_url.clone(), config.webhook_secret.clone());

        Self {
            db,
            rpc_client,
            config,
            energy_token_mint,
            retry_count: Arc::new(AtomicU64::new(0)),
            replay_status: Arc::new(Mutex::new(None)),
            webhook_service,
        }
    }

    /// Start the event processor service
    pub async fn start(&self) {
        if !self.config.enabled {
            info!("Event processor service is disabled");
            return;
        }

        info!(
            "Starting event processor service with interval: {}s",
            self.config.polling_interval_secs
        );

        // Start WebSocket listener if enabled (future enhancement)
        // For now, we'll stick to polling as the primary mechanism
        // self.start_websocket_listener().await;

        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.process_pending_transactions().await {
                error!("Error processing pending transactions: {}", e);
            }
        }
    }

    /// Process pending transactions that need confirmation
    async fn process_pending_transactions(&self) -> Result<()> {
        debug!("Processing pending transactions");

        // Get pending minted readings that need confirmation
        let pending_readings = sqlx::query!(
            r#"
            SELECT id, mint_tx_signature, wallet_address, kwh_amount
            FROM meter_readings
            WHERE minted = true 
              AND on_chain_confirmed = false
              AND mint_tx_signature IS NOT NULL
              AND mint_tx_signature != 'mock_signature'
            ORDER BY submitted_at ASC
            LIMIT $1
            "#,
            self.config.batch_size as i64
        )
        .fetch_all(&*self.db)
        .await?;

        if pending_readings.is_empty() {
            debug!("No pending transactions to process");
            return Ok(());
        }

        info!(
            "Found {} pending transactions to confirm",
            pending_readings.len()
        );

        let mut confirmed_count = 0;
        let mut failed_count = 0;

        for reading in pending_readings {
            let signature_str = match &reading.mint_tx_signature {
                Some(sig) => sig.clone(),
                None => continue, // Skip readings without signatures
            };

            // Skip mock signatures
            if signature_str == "mock_signature" {
                continue;
            }

            match self.confirm_transaction(&signature_str).await {
                Ok(confirmed) => {
                    if confirmed {
                        info!("Transaction confirmed: {}", signature_str);

                        // Mark as confirmed in database
                        if let Err(e) = self
                            .mark_transaction_confirmed(reading.id, &signature_str)
                            .await
                        {
                            error!("Failed to mark transaction as confirmed: {}", e);
                            failed_count += 1;
                        } else {
                            confirmed_count += 1;
                        }
                    } else {
                        debug!("Transaction not yet confirmed: {}", signature_str);
                    }
                }
                Err(e) => {
                    warn!("Error checking transaction {}: {}", signature_str, e);
                    failed_count += 1;
                }
            }
        }

        if confirmed_count > 0 || failed_count > 0 {
            info!(
                "Processed batch: {} confirmed, {} failed",
                confirmed_count, failed_count
            );
        }

        Ok(())
    }

    /// Confirm a transaction on the blockchain with retry logic
    async fn confirm_transaction(&self, signature_str: &str) -> Result<bool> {
        let signature = Signature::from_str(signature_str)?;
        let mut attempts = 0;
        let mut backoff = Duration::from_millis(500); // Initial backoff 500ms

        loop {
            attempts += 1;

            // Get transaction with full details
            match self
                .rpc_client
                .get_transaction(&signature, UiTransactionEncoding::Json)
            {
                Ok(tx) => {
                    // Check if transaction is confirmed
                    if let Some(meta) = &tx.transaction.meta {
                        if meta.err.is_none() {
                            // Transaction succeeded

                            // Parse and store event
                            if let Err(e) = self
                                .parse_and_store_event(tx.slot, tx.block_time, signature_str)
                                .await
                            {
                                warn!("Failed to parse event from transaction: {}", e);
                            }

                            return Ok(true);
                        } else {
                            // Transaction failed
                            warn!("Transaction {} failed: {:?}", signature_str, meta.err);
                            return Ok(false);
                        }
                    }
                    return Ok(false);
                }
                Err(e) => {
                    // Transaction not found or RPC error
                    if attempts >= self.config.max_retries {
                        warn!(
                            "Failed to confirm transaction {} after {} attempts: {}",
                            signature_str, attempts, e
                        );
                        return Ok(false);
                    }

                    debug!(
                        "Error checking transaction {} (attempt {}/{}): {}. Retrying in {:?}...",
                        signature_str, attempts, self.config.max_retries, e, backoff
                    );

                    self.retry_count.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, Duration::from_secs(5));
                    // Cap at 5s
                }
            }
        }
    }

    /// Parse transaction and store event
    async fn parse_and_store_event(
        &self,
        slot: u64,
        block_time: Option<i64>,
        signature: &str,
    ) -> Result<()> {
        // Extract slot and block time
        // let slot = tx.slot;
        // let block_time = tx.block_time;

        // For now, create a simple mint event
        // In production, you'd parse the transaction logs to extract detailed event data
        let event_data = serde_json::json!({
            "signature": signature,
            "slot": slot,
            "block_time": block_time,
            "status": "confirmed"
        });

        // Store event in database
        sqlx::query!(
            r#"
            INSERT INTO blockchain_events 
            (event_type, transaction_signature, slot, block_time, program_id, event_data, processed)
            VALUES ($1, $2, $3, to_timestamp($4), $5, $6, true)
            ON CONFLICT (transaction_signature, event_type) DO NOTHING
            "#,
            EventType::TokenMint.as_str(),
            signature,
            slot as i64,
            block_time.map(|t| t as f64),
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb", // Token-2022 program
            event_data
        )
        .execute(&*self.db)
        .await?;

        info!("Stored blockchain event for transaction: {}", signature);

        // Send webhook notification
        if let Err(e) = self
            .webhook_service
            .send_webhook(EventType::TokenMint.as_str(), event_data)
            .await
        {
            warn!(
                "Failed to send webhook for transaction {}: {}",
                signature, e
            );
        }

        Ok(())
    }

    /// Mark transaction as confirmed in meter_readings
    async fn mark_transaction_confirmed(
        &self,
        reading_id: uuid::Uuid,
        signature: &str,
    ) -> Result<()> {
        // Get transaction details for slot
        let sig = Signature::from_str(signature)?;
        let tx = self
            .rpc_client
            .get_transaction(&sig, UiTransactionEncoding::Json)?;

        sqlx::query!(
            r#"
            UPDATE meter_readings
            SET on_chain_confirmed = true,
            on_chain_slot = $1,
            on_chain_confirmed_at = NOW()
            WHERE id = $2
            "#,
            tx.slot as i64,
            reading_id
        )
        .execute(&*self.db)
        .await?;

        info!(
            "Marked reading {} as on-chain confirmed at slot {}",
            reading_id, tx.slot
        );

        Ok(())
    }

    /// Replay events from a specific slot range
    pub async fn replay_events(&self, start_slot: u64, end_slot: Option<u64>) -> Result<String> {
        let end_slot = end_slot.unwrap_or_else(|| {
            // Default to current slot if not provided
            // We'll just use a reasonable lookahead or fetch current slot
            start_slot + 1000
        });

        info!(
            "Starting event replay from slot {} to {}",
            start_slot, end_slot
        );

        let service = self.clone();

        // Initialize status
        {
            let mut status = match self.replay_status.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("replay_status mutex was poisoned, recovering...");
                    poisoned.into_inner()
                }
            };
            *status = Some(ReplayStatus {
                start_slot,
                end_slot,
                current_slot: start_slot,
                start_time: Utc::now(),
                status: "running".to_string(),
            });
        }

        tokio::spawn(async move {
            let mut current_slot = start_slot;
            while current_slot <= end_slot {
                // Update status periodically
                if current_slot % 10 == 0 {
                    if let Ok(mut status) = service.replay_status.lock() {
                        if let Some(s) = status.as_mut() {
                            s.current_slot = current_slot;
                        }
                    }
                }

                match service.rpc_client.get_block(current_slot) {
                    Ok(block) => {
                        debug!("Processing block {}", current_slot);

                        // Iterate through transactions in the block
                        for tx in block.transactions {
                            // Extract signature
                            let signature = match &tx.transaction {
                                solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
                                    ui_tx.signatures.first().cloned()
                                }
                                _ => None, // Skip binary encoding for now or handle if needed
                            };

                            if let Some(sig) = signature {
                                // Check if transaction mentions our energy token mint
                                // This is a simplified check; in production we'd need more robust filtering
                                // For now, we'll try to parse every confirmed transaction

                                if let Some(meta) = &tx.meta {
                                    if meta.err.is_none() {
                                        // Store event
                                        if let Err(e) = service
                                            .parse_and_store_event(
                                                current_slot,
                                                block.block_time,
                                                &sig,
                                            )
                                            .await
                                        {
                                            warn!("Failed to store replay event {}: {}", sig, e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Block might be skipped or missing, which is common
                        debug!("Skipping block {}: {}", current_slot, e);
                    }
                }
                current_slot += 1;

                // Yield occasionally to avoid blocking
                if current_slot % 100 == 0 {
                    tokio::task::yield_now().await;
                }
            }

            // Update status to completed
            if let Ok(mut status) = service.replay_status.lock() {
                if let Some(s) = status.as_mut() {
                    s.current_slot = end_slot;
                    s.status = "completed".to_string();
                }
            }

            info!(
                "Event replay completed for range {}-{}",
                start_slot, end_slot
            );
        });

        Ok(format!(
            "Replay job started for slots {}-{}",
            start_slot, end_slot
        ))
    }

    /// Get replay status
    pub fn get_replay_status(&self) -> Option<ReplayStatus> {
        match self.replay_status.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                warn!("replay_status mutex was poisoned, recovering...");
                poisoned.into_inner().clone()
            }
        }
    }

    /// Get processing statistics
    pub async fn get_stats(&self) -> Result<EventProcessorStats> {
        let total_events = sqlx::query_scalar!("SELECT COUNT(*) FROM blockchain_events")
            .fetch_one(&*self.db)
            .await?
            .unwrap_or(0);

        let confirmed_readings = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM meter_readings WHERE on_chain_confirmed = true"
        )
        .fetch_one(&*self.db)
        .await?
        .unwrap_or(0);

        let pending_confirmations = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) FROM meter_readings 
            WHERE minted = true 
              AND on_chain_confirmed = false
              AND mint_tx_signature IS NOT NULL
              AND mint_tx_signature != 'mock_signature'
            "#
        )
        .fetch_one(&*self.db)
        .await?
        .unwrap_or(0);

        Ok(EventProcessorStats {
            total_events,
            confirmed_readings,
            pending_confirmations,
            total_retries: self.retry_count.load(Ordering::Relaxed),
        })
    }
}
