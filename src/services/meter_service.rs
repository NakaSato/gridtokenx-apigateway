use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{debug, info, warn};
use uuid::Uuid;
use bigdecimal::BigDecimal;

/// Smart meter reading data
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeterReading {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    pub kwh_amount: Option<BigDecimal>,
    pub reading_timestamp: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub minted: Option<bool>,
    pub mint_tx_signature: Option<String>,
    /// NEW: Reference to meter_registry entry (for verified meters)
    pub meter_id: Option<Uuid>,
    /// NEW: Verification status of the meter for this reading
    pub verification_status: Option<String>,
}

/// Request to submit a meter reading
#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitMeterReadingRequest {
    pub wallet_address: String,
    pub kwh_amount: BigDecimal,
    pub reading_timestamp: DateTime<Utc>,
    /// Optional: smart meter signature for verification
    pub meter_signature: Option<String>,
}

/// Service for managing smart meter data
#[derive(Clone)]
pub struct MeterService {
    db_pool: PgPool,
}

impl MeterService {
    /// Create a new meter service
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Submit a new meter reading
    /// Validates the reading and stores it in the database
    pub async fn submit_reading(
        &self,
        user_id: Uuid,
        request: SubmitMeterReadingRequest,
    ) -> Result<MeterReading> {
        self.submit_reading_with_verification(user_id, request, None, "legacy_unverified").await
    }

    /// Submit a new meter reading with verification status
    /// Validates the reading and stores it in the database
    pub async fn submit_reading_with_verification(
        &self,
        user_id: Uuid,
        request: SubmitMeterReadingRequest,
        meter_id: Option<Uuid>,
        verification_status: &str,
    ) -> Result<MeterReading> {
        // Validate reading amount
        use std::str::FromStr;
        if request.kwh_amount <= BigDecimal::from_str("0").unwrap() {
            return Err(anyhow!("kWh amount must be positive"));
        }

        // Validate timestamp (not in future)
        if request.reading_timestamp > Utc::now() {
            return Err(anyhow!("Reading timestamp cannot be in the future"));
        }

        // Check for duplicate readings (same user, similar timestamp)
        self.check_duplicate_reading(user_id, &request.reading_timestamp).await?;

        // Insert reading into database
        let reading = sqlx::query_as!(
            MeterReading,
            r#"
            INSERT INTO meter_readings (
                id, user_id, wallet_address, kwh_amount, 
                reading_timestamp, submitted_at, minted, meter_id, verification_status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING 
                id, user_id, wallet_address, 
                kwh_amount, reading_timestamp, submitted_at, 
                minted, mint_tx_signature, meter_id, verification_status
            "#,
            Uuid::new_v4(),
            user_id,
            request.wallet_address,
            request.kwh_amount,
            request.reading_timestamp,
            Utc::now(),
            false,
            meter_id: None,
            verification_status,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to insert meter reading: {}", e))?;

        info!(
            "Meter reading submitted: user={}, kwh={}, reading_id={}",
            user_id, request.kwh_amount, reading.id
        );

        Ok(reading)
    }

    /// Check for duplicate readings within a time window
    /// Prevents double-claiming for the same time period
    async fn check_duplicate_reading(
        &self,
        user_id: Uuid,
        reading_timestamp: &DateTime<Utc>,
    ) -> Result<()> {
        // Check for readings within ±15 minutes
        let window_start = *reading_timestamp - chrono::Duration::minutes(15);
        let window_end = *reading_timestamp + chrono::Duration::minutes(15);

        let existing = sqlx::query!(
            r#"
            SELECT id FROM meter_readings
            WHERE user_id = $1
            AND reading_timestamp BETWEEN $2 AND $3
            LIMIT 1
            "#,
            user_id,
            window_start,
            window_end,
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to check duplicate readings: {}", e))?;

        if existing.is_some() {
            return Err(anyhow!(
                "Duplicate reading detected within 15-minute window"
            ));
        }

        Ok(())
    }

    /// Get readings for a specific user
    pub async fn get_user_readings(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        sort_by: &str,
        sort_order: &str,
        minted_filter: Option<bool>,
    ) -> Result<Vec<MeterReading>> {
        // Build query with dynamic sorting and filtering
        let query = if let Some(minted) = minted_filter {
            format!(
                r#"
                SELECT 
                    id, user_id, wallet_address, 
                    kwh_amount, reading_timestamp, submitted_at, 
                    minted, mint_tx_signature, meter_id, verification_status
                FROM meter_readings
                WHERE user_id = $1 AND minted = $2
                ORDER BY {} {}
                LIMIT $3 OFFSET $4
                "#,
                sort_by, sort_order
            )
        } else {
            format!(
                r#"
                SELECT 
                    id, user_id, wallet_address, 
                    kwh_amount, reading_timestamp, submitted_at, 
                    minted, mint_tx_signature, meter_id, verification_status
                FROM meter_readings
                WHERE user_id = $1
                ORDER BY {} {}
                LIMIT $2 OFFSET $3
                "#,
                sort_by, sort_order
            )
        };

        let readings = if let Some(minted) = minted_filter {
            sqlx::query_as::<_, MeterReading>(&query)
                .bind(user_id)
                .bind(minted)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user readings: {}", e))?
        } else {
            sqlx::query_as::<_, MeterReading>(&query)
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user readings: {}", e))?
        };

        debug!("Retrieved {} readings for user {}", readings.len(), user_id);

        Ok(readings)
    }

    /// Count total readings for a user (with optional minted filter)
    pub async fn count_user_readings(
        &self,
        user_id: Uuid,
        minted_filter: Option<bool>,
    ) -> Result<i64> {
        let count = if let Some(minted) = minted_filter {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM meter_readings WHERE user_id = $1 AND minted = $2"
            )
            .bind(user_id)
            .bind(minted)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user readings: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM meter_readings WHERE user_id = $1"
            )
            .bind(user_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user readings: {}", e))?
        };

        Ok(count)
    }

    /// Get readings by wallet address
    pub async fn get_readings_by_wallet(
        &self,
        wallet_address: &str,
        limit: i64,
        offset: i64,
        sort_by: &str,
        sort_order: &str,
        minted_filter: Option<bool>,
    ) -> Result<Vec<MeterReading>> {
        // Build query with dynamic sorting and filtering
        let query = if let Some(minted) = minted_filter {
            format!(
                r#"
                SELECT 
                    id, user_id, wallet_address, 
                    kwh_amount, reading_timestamp, submitted_at, 
                    minted, mint_tx_signature, meter_id, verification_status
                FROM meter_readings
                WHERE wallet_address = $1 AND minted = $2
                ORDER BY {} {}
                LIMIT $3 OFFSET $4
                "#,
                sort_by, sort_order
            )
        } else {
            format!(
                r#"
                SELECT 
                    id, user_id, wallet_address, 
                    kwh_amount, reading_timestamp, submitted_at, 
                    minted, mint_tx_signature, meter_id, verification_status
                FROM meter_readings
                WHERE wallet_address = $1
                ORDER BY {} {}
                LIMIT $2 OFFSET $3
                "#,
                sort_by, sort_order
            )
        };

        let readings = if let Some(minted) = minted_filter {
            sqlx::query_as::<_, MeterReading>(&query)
                .bind(wallet_address)
                .bind(minted)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch wallet readings: {}", e))?
        } else {
            sqlx::query_as::<_, MeterReading>(&query)
                .bind(wallet_address)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch wallet readings: {}", e))?
        };

        debug!("Retrieved {} readings for wallet {}", readings.len(), wallet_address);

        Ok(readings)
    }

    /// Count total readings for a wallet (with optional minted filter)
    pub async fn count_wallet_readings(
        &self,
        wallet_address: &str,
        minted_filter: Option<bool>,
    ) -> Result<i64> {
        let count = if let Some(minted) = minted_filter {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM meter_readings WHERE wallet_address = $1 AND minted = $2"
            )
            .bind(wallet_address)
            .bind(minted)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count wallet readings: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM meter_readings WHERE wallet_address = $1"
            )
            .bind(wallet_address)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count wallet readings: {}", e))?
        };

        Ok(count)
    }

    /// Get unminted readings (readings that haven't been converted to tokens yet)
    pub async fn get_unminted_readings(&self, limit: i64) -> Result<Vec<MeterReading>> {
        let readings = sqlx::query_as!(
            MeterReading,
            r#"
            SELECT 
                id, user_id, wallet_address, 
                kwh_amount, reading_timestamp, submitted_at, 
                minted, mint_tx_signature, meter_id, verification_status
            FROM meter_readings
            WHERE minted = false
            ORDER BY submitted_at ASC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch unminted readings: {}", e))?;

        debug!("Retrieved {} unminted readings", readings.len());

        Ok(readings)
    }

    /// Mark a reading as minted
    pub async fn mark_as_minted(
        &self,
        reading_id: Uuid,
        tx_signature: &str,
    ) -> Result<MeterReading> {
        let reading = sqlx::query_as!(
            MeterReading,
            r#"
            UPDATE meter_readings
            SET minted = true, mint_tx_signature = $2
            WHERE id = $1
            RETURNING 
                id, user_id, wallet_address, 
                kwh_amount, reading_timestamp, submitted_at, 
                minted, mint_tx_signature, meter_id, verification_status
            "#,
            reading_id,
            tx_signature,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to mark reading as minted: {}", e))?;

        info!(
            "Marked reading {} as minted with tx: {}",
            reading_id, tx_signature
        );

        Ok(reading)
    }

    /// Get a specific reading by ID
    pub async fn get_reading_by_id(&self, reading_id: Uuid) -> Result<MeterReading> {
        let reading = sqlx::query_as!(
            MeterReading,
            r#"
            SELECT 
                id, user_id, wallet_address, 
                kwh_amount, reading_timestamp, submitted_at, 
                minted, mint_tx_signature, meter_id, verification_status
            FROM meter_readings
            WHERE id = $1
            "#,
            reading_id,
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch reading: {}", e))?
        .ok_or_else(|| anyhow!("Reading not found"))?;

        Ok(reading)
    }

    /// Calculate total unminted kWh for a user
    pub async fn get_unminted_total(&self, user_id: Uuid) -> Result<BigDecimal> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(kwh_amount), 0) as "total!"
            FROM meter_readings
            WHERE user_id = $1 AND minted = false
            "#,
            user_id,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to calculate unminted total: {}", e))?;

        Ok(result.total)
    }

    /// Get total minted kWh for a user
    pub async fn get_minted_total(&self, user_id: Uuid) -> Result<BigDecimal> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(kwh_amount), 0) as "total!"
            FROM meter_readings
            WHERE user_id = $1 AND minted = true
            "#,
            user_id,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to calculate minted total: {}", e))?;

        Ok(result.total)
    }

    /// Validate meter reading data
    pub fn validate_reading(request: &SubmitMeterReadingRequest) -> Result<()> {
        // Amount validation
        use std::str::FromStr;
        if request.kwh_amount <= BigDecimal::from_str("0").unwrap() {
            return Err(anyhow!("kWh amount must be positive"));
        }

        // Maximum reasonable amount (e.g., 100 kWh per reading)
        let max_kwh = BigDecimal::from(100);
        if request.kwh_amount > max_kwh {
            warn!("Unusually high kWh reading: {}", request.kwh_amount);
            return Err(anyhow!(
                "kWh amount exceeds maximum ({} kWh)",
                max_kwh
            ));
        }

        // Timestamp validation
        if request.reading_timestamp > Utc::now() {
            return Err(anyhow!("Reading timestamp cannot be in the future"));
        }

        // Not too old (e.g., within last 7 days)
        let max_age = Utc::now() - chrono::Duration::days(7);
        if request.reading_timestamp < max_age {
            return Err(anyhow!("Reading timestamp is too old (>7 days)"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_reading_positive_amount() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: BigDecimal::from(10),
            reading_timestamp: Utc::now(),
            meter_signature: None,
        };

        assert!(MeterService::validate_reading(&request).is_ok());
    }

    #[test]
    fn test_validate_reading_zero_amount() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: BigDecimal::from(0),
            reading_timestamp: Utc::now(),
            meter_signature: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }

    #[test]
    fn test_validate_reading_future_timestamp() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: BigDecimal::from(10),
            reading_timestamp: Utc::now() + chrono::Duration::hours(1),
            meter_signature: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }

    #[test]
    fn test_validate_reading_excessive_amount() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: BigDecimal::from(150), // Over 100 kWh limit
            reading_timestamp: Utc::now(),
            meter_signature: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }

    #[test]
    fn test_validate_reading_old_timestamp() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: BigDecimal::from(10),
            reading_timestamp: Utc::now() - chrono::Duration::days(10),
            meter_signature: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }
}
