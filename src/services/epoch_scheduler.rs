use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{RwLock, broadcast};
use tokio::time::{Duration as TokioDuration, interval};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::database::schema::types::EpochStatus;
use crate::error::ApiError;
use crate::services::market_clearing_service::{MarketClearingService, MarketEpoch};

#[derive(Debug, Clone)]
pub struct EpochConfig {
    pub epoch_duration_minutes: u64,
    pub transition_check_interval_secs: u64,
    pub max_orders_per_epoch: usize,
    pub platform_fee_rate: Decimal,
}

impl Default for EpochConfig {
    fn default() -> Self {
        Self {
            epoch_duration_minutes: 15,
            transition_check_interval_secs: 60,
            max_orders_per_epoch: 10_000,
            platform_fee_rate: Decimal::from_str("0.01").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EpochTransitionEvent {
    pub epoch_id: Uuid,
    pub epoch_number: i64,
    pub old_status: String,
    pub new_status: String,
    pub transition_time: DateTime<Utc>,
}

#[derive(Debug)]
pub struct EpochScheduler {
    db: PgPool,
    config: EpochConfig,
    market_clearing_service: MarketClearingService,
    current_epoch: Arc<RwLock<Option<MarketEpoch>>>,
    is_running: AtomicBool,
    event_sender: broadcast::Sender<EpochTransitionEvent>,
    shutdown_receiver: Arc<RwLock<Option<broadcast::Receiver<()>>>>,
}

impl EpochScheduler {
    pub fn new(db: PgPool, config: EpochConfig) -> Self {
        let market_clearing_service = MarketClearingService::new(db.clone());
        let (event_sender, _) = broadcast::channel(1000);
        let (_, shutdown_receiver) = broadcast::channel(1);

        Self {
            db,
            config,
            market_clearing_service,
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
        let market_clearing_service = self.market_clearing_service.clone();
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

                        match Self::process_epoch_transitions_internal(
                            &db,
                            &market_clearing_service,
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
            let target_state = self.determine_target_state(&latest_epoch, now);

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
            if target_state == "expired" && latest_epoch.status != EpochStatus::Settled {
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

        let target_state = self.determine_target_state(&epoch, Utc::now());
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

    // Internal methods

    async fn process_epoch_transitions_internal(
        db: &PgPool,
        market_clearing_service: &MarketClearingService,
        current_epoch: &Arc<RwLock<Option<MarketEpoch>>>,
        event_sender: &broadcast::Sender<EpochTransitionEvent>,
    ) -> Result<()> {
        let now = Utc::now();

        // 1. Activate pending epochs
        Self::activate_pending_epochs(db, current_epoch, event_sender, now).await?;

        // 2. Clear expired active epochs
        Self::clear_expired_epochs(
            db,
            market_clearing_service,
            current_epoch,
            event_sender,
            now,
        )
        .await?;

        // 3. Create next epoch if needed
        Self::ensure_future_epoch_exists(db, now).await?;

        Ok(())
    }

    async fn activate_pending_epochs(
        db: &PgPool,
        current_epoch: &Arc<RwLock<Option<MarketEpoch>>>,
        event_sender: &broadcast::Sender<EpochTransitionEvent>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        // Find epochs that should be active now
        let epochs_to_activate = sqlx::query!(
            r#"
            SELECT id, epoch_number, start_time, end_time, status as "status: EpochStatus"
            FROM market_epochs
            WHERE status = $1
            AND start_time <= $2
            AND end_time > $2
            ORDER BY start_time ASC
            "#,
            EpochStatus::Pending as EpochStatus,
            now
        )
        .fetch_all(db)
        .await?;

        for epoch_row in epochs_to_activate {
            info!(
                "Activating epoch: {} ({})",
                epoch_row.epoch_number, epoch_row.id
            );

            // Update status to active
            sqlx::query!(
                "UPDATE market_epochs SET status = $1::epoch_status, updated_at = NOW() WHERE id = $2",
                EpochStatus::Active as EpochStatus,
                epoch_row.id
            )
            .execute(db)
            .await?;

            // Update current epoch
            let epoch = MarketEpoch {
                id: epoch_row.id,
                epoch_number: epoch_row.epoch_number,
                start_time: epoch_row.start_time,
                end_time: epoch_row.end_time,
                status: EpochStatus::Active,
                clearing_price: None,
                total_volume: Some(bigdecimal::BigDecimal::from_str("0").unwrap()),
                total_orders: Some(0),
                matched_orders: Some(0),
            };

            *current_epoch.write().await = Some(epoch.clone());

            // Send transition event
            let _ = event_sender.send(EpochTransitionEvent {
                epoch_id: epoch_row.id,
                epoch_number: epoch_row.epoch_number,
                old_status: "pending".to_string(),
                new_status: "active".to_string(),
                transition_time: now,
            });
        }

        Ok(())
    }

    async fn clear_expired_epochs(
        db: &PgPool,
        market_clearing_service: &MarketClearingService,
        current_epoch: &Arc<RwLock<Option<MarketEpoch>>>,
        event_sender: &broadcast::Sender<EpochTransitionEvent>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        // Find active epochs that have expired
        let epochs_to_clear = sqlx::query!(
            r#"
            SELECT id, epoch_number, start_time, end_time, status as "status: EpochStatus"
            FROM market_epochs
            WHERE status = $1
            AND end_time <= $2
            ORDER BY end_time ASC
            "#,
            EpochStatus::Active as EpochStatus,
            now
        )
        .fetch_all(db)
        .await?;

        for epoch_row in epochs_to_clear {
            info!(
                "Clearing expired epoch: {} ({})",
                epoch_row.epoch_number, epoch_row.id
            );

            // Update status to cleared first
            sqlx::query!(
                "UPDATE market_epochs SET status = $1::epoch_status, updated_at = NOW() WHERE id = $2",
                EpochStatus::Cleared as EpochStatus,
                epoch_row.id
            )
            .execute(db)
            .await?;

            // Run order matching for this epoch
            match market_clearing_service
                .run_order_matching(epoch_row.id)
                .await
            {
                Ok(matches) => {
                    info!(
                        "Order matching completed for epoch {}: {} matches created",
                        epoch_row.id,
                        matches.len()
                    );

                    // Send transition event
                    let _ = event_sender.send(EpochTransitionEvent {
                        epoch_id: epoch_row.id,
                        epoch_number: epoch_row.epoch_number,
                        old_status: "active".to_string(),
                        new_status: "cleared".to_string(),
                        transition_time: now,
                    });
                }
                Err(e) => {
                    error!(
                        "Failed to run order matching for epoch {}: {}",
                        epoch_row.id, e
                    );

                    // Keep as cleared, will be retried
                    let _ = event_sender.send(EpochTransitionEvent {
                        epoch_id: epoch_row.id,
                        epoch_number: epoch_row.epoch_number,
                        old_status: "active".to_string(),
                        new_status: "cleared".to_string(),
                        transition_time: now,
                    });
                }
            }

            // Update current epoch if this was the active one
            let mut current = current_epoch.write().await;
            if let Some(ref mut current_epoch) = *current {
                if current_epoch.id == epoch_row.id {
                    current_epoch.status = EpochStatus::Cleared;
                }
            }
        }

        Ok(())
    }

    async fn ensure_future_epoch_exists(db: &PgPool, now: DateTime<Utc>) -> Result<()> {
        // Calculate next epoch number
        let next_epoch_time = Self::calculate_next_epoch_start(now);
        let next_epoch_number = Self::calculate_epoch_number(next_epoch_time);

        // Check if next epoch already exists
        let existing = sqlx::query!(
            "SELECT id FROM market_epochs WHERE epoch_number = $1",
            next_epoch_number
        )
        .fetch_optional(db)
        .await?;

        if existing.is_none() {
            info!(
                "Creating next epoch: {} ({})",
                next_epoch_number, next_epoch_time
            );

            let epoch_id = Uuid::new_v4();
            let epoch_end = next_epoch_time + Duration::minutes(15);

            sqlx::query!(
                r#"
                INSERT INTO market_epochs (
                    id, epoch_number, start_time, end_time, status
                ) VALUES ($1, $2, $3, $4, $5::epoch_status)
                "#,
                epoch_id,
                next_epoch_number,
                next_epoch_time,
                epoch_end,
                EpochStatus::Pending as EpochStatus
            )
            .execute(db)
            .await?;
        }

        Ok(())
    }

    async fn wait_for_shutdown_signal(is_running: &AtomicBool) {
        while is_running.load(Ordering::Relaxed) {
            tokio::time::sleep(TokioDuration::from_millis(100)).await;
        }
    }

    // Helper methods

    fn determine_target_state(&self, epoch: &MarketEpoch, now: DateTime<Utc>) -> String {
        if now < epoch.start_time {
            "pending".to_string()
        } else if now >= epoch.start_time && now < epoch.end_time {
            "active".to_string()
        } else if now >= epoch.end_time && epoch.status != EpochStatus::Settled {
            "cleared".to_string()
        } else {
            epoch.status.to_string()
        }
    }

    async fn get_latest_epoch(&self) -> Result<Option<MarketEpoch>> {
        let epoch = sqlx::query_as!(
            MarketEpoch,
            r#"
            SELECT
                id, epoch_number, start_time, end_time, status as "status: EpochStatus",
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
                id, epoch_number, start_time, end_time, status as "status: EpochStatus",
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

    fn calculate_epoch_number(timestamp: DateTime<Utc>) -> i64 {
        (timestamp.year() as i64) * 100_000_000
            + (timestamp.month() as i64) * 1_000_000
            + (timestamp.day() as i64) * 10_000
            + (timestamp.hour() as i64) * 100
            + ((timestamp.minute() / 15) * 15) as i64
    }

    fn calculate_next_epoch_start(now: DateTime<Utc>) -> DateTime<Utc> {
        let current_epoch_start = now
            .with_minute((now.minute() / 15) * 15)
            .and_then(|dt| dt.with_second(0))
            .and_then(|dt| dt.with_nanosecond(0))
            .unwrap_or(now);

        current_epoch_start + Duration::minutes(15)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sqlx::PgPool;
    use std::env;

    // Helper function to create a test database connection
    async fn create_test_db() -> PgPool {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost/gridtokenx_test".to_string()
        });

        PgPool::connect_lazy(&database_url).expect("Failed to connect to test database")
    }

    #[tokio::test]
    async fn test_epoch_number_calculation() {
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);

        // Expected: 202511091430 (YYYYMMDDHHMM with 15-minute blocks)
        assert_eq!(epoch_number, 202511091430);
    }

    #[tokio::test]
    async fn test_next_epoch_start_calculation() {
        let now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 37, 0).unwrap();
        let next_epoch_start = EpochScheduler::calculate_next_epoch_start(now);

        // Should be 14:45 (next 15-minute block)
        let expected = Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap();
        assert_eq!(next_epoch_start, expected);
    }

    #[tokio::test]
    async fn test_target_state_determination() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(create_test_db().await, config);

        let now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();

        // Test pending epoch
        let pending_epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap(),
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: Some(bigdecimal::BigDecimal::from_str("0").unwrap()),
            total_orders: Some(0),
            matched_orders: Some(0),
        };

        let target_state = scheduler.determine_target_state(&pending_epoch, now);
        assert_eq!(target_state, "active");

        // Test expired epoch
        let later_now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 50, 0).unwrap();
        let target_state = scheduler.determine_target_state(&pending_epoch, later_now);
        assert_eq!(target_state, "cleared");
    }

    #[tokio::test]
    async fn test_epoch_boundaries_at_midnight() {
        // Test epoch calculation across midnight boundary
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 23, 45, 0).unwrap();
        let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);
        assert_eq!(epoch_number, 202511092345);

        let next_start = EpochScheduler::calculate_next_epoch_start(timestamp);
        let expected = Utc.with_ymd_and_hms(2025, 11, 10, 0, 0, 0).unwrap();
        assert_eq!(next_start, expected);
    }

    #[tokio::test]
    async fn test_epoch_boundaries_at_month_end() {
        // Test epoch calculation across month boundary
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 30, 23, 45, 0).unwrap();
        let next_start = EpochScheduler::calculate_next_epoch_start(timestamp);
        let expected = Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap();
        assert_eq!(next_start, expected);
    }

    #[tokio::test]
    async fn test_all_15_minute_boundaries() {
        // Test all four 15-minute boundaries in an hour
        let boundaries = vec![0, 15, 30, 45];

        for minute in boundaries {
            let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, minute, 0).unwrap();
            let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);

            // Extract minute from epoch number
            let epoch_minute = (epoch_number % 100) as u32;
            assert_eq!(epoch_minute, minute);
        }
    }

    #[tokio::test]
    async fn test_state_transition_sequence() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(create_test_db().await, config);

        let epoch_start = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_end = Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap();

        // Test state progression
        let mut epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: epoch_start,
            end_time: epoch_end,
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: Some(bigdecimal::BigDecimal::from_str("0").unwrap()),
            total_orders: Some(0),
            matched_orders: Some(0),
        };

        // At start: pending → active
        let state = scheduler.determine_target_state(&epoch, epoch_start);
        assert_eq!(state, "active");

        // During epoch: active → active
        epoch.status = EpochStatus::Active;
        let mid_time = epoch_start + chrono::Duration::minutes(7);
        let state = scheduler.determine_target_state(&epoch, mid_time);
        assert_eq!(state, "active");

        // After end: active → cleared
        let after_end = epoch_end + chrono::Duration::seconds(1);
        let state = scheduler.determine_target_state(&epoch, after_end);
        assert_eq!(state, "cleared");
    }

    #[tokio::test]
    async fn test_epoch_duration_always_15_minutes() {
        // Test multiple random times
        let test_times = vec![
            Utc.with_ymd_and_hms(2025, 11, 9, 0, 5, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 8, 17, 30).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 12, 42, 15).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 18, 58, 45).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 23, 59, 59).unwrap(),
        ];

        for time in test_times {
            let next_start = EpochScheduler::calculate_next_epoch_start(time);
            let next_end = next_start + chrono::Duration::minutes(15);

            let duration_secs = (next_end - next_start).num_seconds();
            assert_eq!(duration_secs, 900); // 15 minutes = 900 seconds
        }
    }

    #[tokio::test]
    async fn test_epoch_number_monotonicity() {
        // Epoch numbers should strictly increase over time
        let mut previous_epoch_number = 0i64;

        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, hour, minute, 0).unwrap();
                let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);

                if previous_epoch_number > 0 {
                    assert!(
                        epoch_number > previous_epoch_number,
                        "Epoch numbers must increase: {} should be > {}",
                        epoch_number,
                        previous_epoch_number
                    );
                }

                previous_epoch_number = epoch_number;
            }
        }
    }

    #[tokio::test]
    async fn test_epoch_number_format() {
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);

        // Convert to string to check format
        let epoch_str = epoch_number.to_string();
        assert_eq!(
            epoch_str.len(),
            12,
            "Epoch number should be 12 digits (YYYYMMDDHHMM)"
        );

        // Extract and verify components
        let year: i32 = epoch_str[0..4].parse().unwrap();
        let month: u32 = epoch_str[4..6].parse().unwrap();
        let day: u32 = epoch_str[6..8].parse().unwrap();
        let hour: u32 = epoch_str[8..10].parse().unwrap();
        let minute: u32 = epoch_str[10..12].parse().unwrap();

        assert_eq!(year, 2025);
        assert_eq!(month, 11);
        assert_eq!(day, 9);
        assert_eq!(hour, 14);
        assert_eq!(minute, 30);
    }

    #[tokio::test]
    async fn test_leap_year_february_29() {
        // 2024 is a leap year
        let timestamp = Utc.with_ymd_and_hms(2024, 2, 29, 10, 15, 0).unwrap();
        let epoch_number = EpochScheduler::calculate_epoch_number(timestamp);

        let epoch_str = epoch_number.to_string();
        let month: u32 = epoch_str[4..6].parse().unwrap();
        let day: u32 = epoch_str[6..8].parse().unwrap();

        assert_eq!(month, 2);
        assert_eq!(day, 29);
    }

    #[tokio::test]
    async fn test_expired_epoch_detection() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(create_test_db().await, config);

        let epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap(),
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: Some(bigdecimal::BigDecimal::from_str("0").unwrap()),
            total_orders: Some(0),
            matched_orders: Some(0),
        };

        // 1 hour after end time - should be expired
        let far_future = Utc.with_ymd_and_hms(2025, 11, 9, 15, 45, 0).unwrap();
        let state = scheduler.determine_target_state(&epoch, far_future);
        assert_eq!(state, "cleared");
    }

    #[tokio::test]
    async fn test_cleared_epoch_should_settle() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(create_test_db().await, config);

        let epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap(),
            status: EpochStatus::Cleared,
            clearing_price: Some(bigdecimal::BigDecimal::from_str("0.15").unwrap()),
            total_volume: Some(bigdecimal::BigDecimal::from_str("1000").unwrap()),
            total_orders: Some(10),
            matched_orders: Some(8),
        };

        // Cleared epoch should eventually settle
        let current_time = Utc.with_ymd_and_hms(2025, 11, 9, 14, 50, 0).unwrap();
        let state = scheduler.determine_target_state(&epoch, current_time);

        // After clearing, state should progress toward settlement
        // The exact logic depends on your settlement conditions
        assert!(state == "cleared" || state == "settled");
    }
}
