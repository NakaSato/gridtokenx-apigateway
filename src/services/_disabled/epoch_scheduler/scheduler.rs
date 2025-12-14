use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::market_clearing::{MarketClearingService, MarketEpoch};
use crate::services::BlockchainService;

use super::types::{EpochConfig, EpochTransitionEvent};
use super::utils::determine_target_state;
use super::worker::process_epoch_transitions_internal;

#[derive(Debug)]
pub struct EpochScheduler {
    db: PgPool,
    config: EpochConfig,
    market_clearing: MarketClearingService,
    #[allow(dead_code)]
    blockchain_service: BlockchainService,
    current_epoch: Arc<RwLock<Option<MarketEpoch>>>,
    is_running: AtomicBool,
    event_sender: broadcast::Sender<EpochTransitionEvent>,
    shutdown_receiver: Arc<RwLock<Option<broadcast::Receiver<()>>>>,
}

impl EpochScheduler {
    pub fn new(db: PgPool, config: EpochConfig, blockchain_service: BlockchainService) -> Self {
        let market_clearing =
            MarketClearingService::new(db.clone(), blockchain_service.clone());
        let (event_sender, _) = broadcast::channel(1000);
        let (_, shutdown_receiver) = broadcast::channel(1);

        Self {
            db,
            config,
            market_clearing,
            blockchain_service,
            current_epoch: Arc::new(RwLock::new(None)),
            is_running: AtomicBool::new(false),
            event_sender,
            shutdown_receiver: Arc::new(RwLock::new(Some(shutdown_receiver))),
        }
    }

    /// Start the epoch scheduler
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<()> {
        if self.is_running.load(Ordering::Relaxed) {
            warn!("Epoch scheduler is already running");
            return Ok(());
        }

        info!(
            "Starting epoch scheduler with configuration: {:?}",
            self.config
        );

        // Recover state on startup
        self.recover_state().await?;

        self.is_running.store(true, Ordering::Relaxed);
        let is_running = Arc::new(AtomicBool::new(true));
        let db = self.db.clone();
        let config = self.config.clone();
        let market_clearing = self.market_clearing.clone();
        let current_epoch = self.current_epoch.clone();
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            let mut interval = interval(TokioDuration::from_secs(
                config.transition_check_interval_secs,
            ));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !is_running.load(Ordering::Relaxed) {
                            break;
                        }

                        match process_epoch_transitions_internal(
                            &db,
                            &market_clearing,
                            &current_epoch,
                            &event_sender,
                        ).await {
                            Ok(_) => {
                                debug!("Epoch transition processing completed successfully");
                            }
                            Err(e) => {
                                error!("Error during epoch transition processing: {}", e);
                            }
                        }
                    }
                    _ = Self::wait_for_shutdown_signal(&is_running) => {
                        info!("Epoch scheduler shutdown signal received");
                        break;
                    }
                }
            }

            info!("Epoch scheduler stopped");
        });

        info!("Epoch scheduler started successfully");
        Ok(())
    }

    /// Stop the epoch scheduler
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping epoch scheduler...");
        self.is_running.store(false, Ordering::Relaxed);

        // Send shutdown signal
        let mut shutdown_receiver = self.shutdown_receiver.write().await;
        if let Some(receiver) = shutdown_receiver.take() {
            drop(receiver);
        }

        info!("Epoch scheduler stopped");
        Ok(())
    }

    /// Recover scheduler state after server restart
    #[instrument(skip(self))]
    pub async fn recover_state(&self) -> Result<()> {
        info!("Recovering epoch scheduler state...");

        // Find the most recent epoch
        let latest_epoch = self.get_latest_epoch().await?;

        if let Some(latest_epoch) = latest_epoch {
            let now = Utc::now();
            let target_state = determine_target_state(&latest_epoch, now);

            info!(
                "Latest epoch: {} ({}), target state: {}",
                latest_epoch.epoch_number, latest_epoch.status, target_state
            );

            // Update epoch status if needed
            if target_state != latest_epoch.status.to_string() {
                self.update_epoch_status(latest_epoch.id, &target_state)
                    .await?;

                // Send transition event
                let _ = self.event_sender.send(EpochTransitionEvent {
                    epoch_id: latest_epoch.id,
                    epoch_number: latest_epoch.epoch_number,
                    old_status: latest_epoch.status.to_string(),
                    new_status: target_state.clone(),
                    transition_time: now,
                });
            }

            // Update current epoch
            *self.current_epoch.write().await = Some(latest_epoch.clone());

            // Resume order matching for expired epochs that haven't been processed
            if target_state == "expired"
                && latest_epoch.status != crate::database::schema::types::EpochStatus::Settled
            {
                warn!(
                    "Found expired epoch that needs processing: {}",
                    latest_epoch.id
                );
                // This will be handled by main loop
            }
        } else {
            info!("No existing epochs found, will create first epoch when needed");
        }

        info!("Epoch scheduler state recovery completed");
        Ok(())
    }

    /// Get current active epoch
    pub async fn get_current_epoch(&self) -> Result<Option<MarketEpoch>> {
        let current = self.current_epoch.read().await;
        Ok(current.clone())
    }

    /// Manually trigger epoch transition (for testing)
    pub async fn trigger_epoch_transition(&self, epoch_id: Uuid) -> Result<()> {
        info!("Manually triggering transition for epoch: {}", epoch_id);

        let epoch = self
            .get_epoch_by_id(epoch_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Epoch not found".to_string()))?;

        let target_state = determine_target_state(&epoch, Utc::now());
        if target_state != epoch.status.to_string() {
            self.update_epoch_status(epoch_id, &target_state).await?;

            let _ = self.event_sender.send(EpochTransitionEvent {
                epoch_id,
                epoch_number: epoch.epoch_number,
                old_status: epoch.status.to_string(),
                new_status: target_state,
                transition_time: Utc::now(),
            });
        }

        Ok(())
    }

    /// Subscribe to epoch transition events
    pub fn subscribe_transitions(&self) -> broadcast::Receiver<EpochTransitionEvent> {
        self.event_sender.subscribe()
    }

    async fn wait_for_shutdown_signal(is_running: &AtomicBool) {
        while is_running.load(Ordering::Relaxed) {
            tokio::time::sleep(TokioDuration::from_millis(100)).await;
        }
    }

    // Helper methods

    async fn get_latest_epoch(&self) -> Result<Option<MarketEpoch>> {
        let epoch = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT
                id, epoch_number, start_time, end_time, status as "status: crate::database::schema::types::EpochStatus",
                clearing_price, total_volume, total_orders, matched_orders
            FROM market_epochs
            ORDER BY epoch_number DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(epoch)
    }

    async fn get_epoch_by_id(&self, epoch_id: Uuid) -> Result<Option<MarketEpoch>> {
        let epoch = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT
                id, epoch_number, start_time, end_time, status as "status: crate::database::schema::types::EpochStatus",
                clearing_price, total_volume, total_orders, matched_orders
            FROM market_epochs
            WHERE id = $1
            "#,
            epoch_id
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(epoch)
    }

    async fn update_epoch_status(&self, epoch_id: Uuid, status: &str) -> Result<()> {
        let status_str = match status {
            "pending" => "pending",
            "active" => "active",
            "cleared" => "cleared",
            "settled" => "settled",
            _ => return Err(anyhow::anyhow!("Invalid epoch status: {}", status)),
        };

        // Use raw query to avoid type casting issues
        sqlx::query(&format!("UPDATE market_epochs SET status = '{}'::epoch_status, updated_at = NOW() WHERE id = $1", status_str))
            .bind(epoch_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }
}
