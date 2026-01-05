use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;
use tracing::info;

use crate::database::schema::types::EpochStatus;
use super::MarketClearingService;
use super::types::MarketEpoch;

impl MarketClearingService {
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

    /// Update epoch statistics
    pub(super) async fn update_epoch_statistics(
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
}
