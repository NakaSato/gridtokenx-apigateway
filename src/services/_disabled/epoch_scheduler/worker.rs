use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};
use uuid::Uuid;

use crate::database::schema::types::EpochStatus;
use crate::services::market_clearing::{MarketClearingService, MarketEpoch};

use super::types::EpochTransitionEvent;
use super::utils::{calculate_epoch_number, calculate_next_epoch_start};

pub async fn process_epoch_transitions_internal(
    db: &PgPool,
    market_clearing: &MarketClearingService,
    current_epoch: &Arc<RwLock<Option<MarketEpoch>>>,
    event_sender: &broadcast::Sender<EpochTransitionEvent>,
) -> Result<()> {
    let now = Utc::now();

    // 1. Activate pending epochs
    activate_pending_epochs(db, current_epoch, event_sender, now).await?;

    // 2. Clear expired active epochs
    clear_expired_epochs(
        db,
        market_clearing,
        current_epoch,
        event_sender,
        now,
    )
    .await?;

    // 3. Create next epoch if needed
    ensure_future_epoch_exists(db, now).await?;

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
            total_volume: Some(Decimal::ZERO),
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
    market_clearing: &MarketClearingService,
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
        match market_clearing
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
    let next_epoch_time = calculate_next_epoch_start(now);
    let next_epoch_number = calculate_epoch_number(next_epoch_time);

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
