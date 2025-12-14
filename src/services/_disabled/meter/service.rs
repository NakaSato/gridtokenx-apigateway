use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Smart meter reading data
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeterReading {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    pub kwh_amount: Option<Decimal>,
    pub reading_timestamp: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub minted: Option<bool>,
    pub mint_tx_signature: Option<String>,
    /// NEW: Reference to meter registry entry (for verified meters)
    pub meter_id: Option<Uuid>,
    /// NEW: Meter serial number
    pub meter_serial: Option<String>,
    /// NEW: Verification status of the meter for this reading (as string from database)
    pub verification_status: Option<String>,
}

/// Request to submit a meter reading
#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitMeterReadingRequest {
    pub wallet_address: String,
    pub kwh_amount: Decimal,
    pub reading_timestamp: DateTime<Utc>,
    /// Optional: smart meter signature for verification
    pub meter_signature: Option<String>,
    /// Optional: meter serial number (legacy)
    pub meter_serial: Option<String>,
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
        self.submit_reading_with_verification(user_id, request, None, "legacy_unverified")
            .await
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
        // Allow negative amounts for consumption (burn)
        // if request.kwh_amount <= Decimal::ZERO {
        //     return Err(anyhow!("kWh amount must be positive"));
        // }

        // Validate timestamp (not in future)
        if request.reading_timestamp > Utc::now() {
            return Err(anyhow!("Reading timestamp cannot be in the future"));
        }

        // Check for duplicate readings (same user, similar timestamp)
        self.check_duplicate_reading(
            user_id,
            &request.reading_timestamp,
            meter_id,
            request.meter_serial.as_deref(),
        )
        .await?;

        // Insert reading into database
        let reading_id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO meter_readings (
                id, user_id, wallet_address, kwh_amount,
                reading_timestamp, timestamp, submitted_at, minted, meter_id, verification_status,
                meter_serial
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING
                id, user_id, wallet_address,
                kwh_amount, reading_timestamp, submitted_at,
                minted, mint_tx_signature, meter_id, verification_status
            "#,
        )
        .bind(reading_id)
        .bind(user_id)
        .bind(&request.wallet_address)
        .bind(&request.kwh_amount)
        .bind(&request.reading_timestamp)
        .bind(&request.reading_timestamp) // Use reading_timestamp for timestamp column
        .bind(&Utc::now())
        .bind(&false)
        .bind(&meter_id)
        .bind(&verification_status)
        .bind(&request.meter_serial)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to insert meter reading: {}", e))?;

        let reading = MeterReading {
            id: row.get("id"),
            user_id: row.get("user_id"),
            wallet_address: row.get("wallet_address"),
            kwh_amount: row.get("kwh_amount"),
            reading_timestamp: row.get("reading_timestamp"),
            submitted_at: row.get("submitted_at"),
            minted: row.get("minted"),
            mint_tx_signature: row.get("mint_tx_signature"),
            meter_id: row.get("meter_id"),
            meter_serial: None, // Not included in RETURNING clause
            verification_status: row.get("verification_status"),
        };

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
        meter_id: Option<Uuid>,
        meter_serial: Option<&str>,
    ) -> Result<()> {
        // Check for readings within ±15 minutes
        let window_start = *reading_timestamp - chrono::Duration::minutes(15);
        let window_end = *reading_timestamp + chrono::Duration::minutes(15);

        let existing: Option<Uuid> = if let Some(mid) = meter_id {
            // Check for duplicate reading for this specific meter (verified)
            let record: Option<Uuid> = sqlx::query_scalar(
                r#"
                SELECT id FROM meter_readings
                WHERE user_id = $1
                AND meter_id = $2
                AND reading_timestamp BETWEEN $3 AND $4
                LIMIT 1
                "#,
            )
            .bind(user_id)
            .bind(mid)
            .bind(window_start)
            .bind(window_end)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to check duplicate readings: {}", e))?;

            record
        } else if let Some(serial) = meter_serial {
            // Check for duplicate reading for this specific meter serial (unverified/legacy)
            let record: Option<Uuid> = sqlx::query_scalar(
                r#"
                SELECT id FROM meter_readings
                WHERE user_id = $1
                AND meter_serial = $2
                AND reading_timestamp BETWEEN $3 AND $4
                LIMIT 1
                "#,
            )
            .bind(user_id)
            .bind(serial)
            .bind(window_start)
            .bind(window_end)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to check duplicate readings: {}", e))?;

            record
        } else {
            // Legacy check: Check for duplicate reading for the user (any meter)
            let record: Option<Uuid> = sqlx::query_scalar(
                r#"
                SELECT id FROM meter_readings
                WHERE user_id = $1
                AND reading_timestamp BETWEEN $2 AND $3
                LIMIT 1
                "#,
            )
            .bind(user_id)
            .bind(window_start)
            .bind(window_end)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to check duplicate readings: {}", e))?;

            record
        };

        if let Some(existing_id) = existing {
            warn!(
                "Duplicate reading check failed. Existing reading ID: {}",
                existing_id
            );
            return Err(anyhow!(
                "Duplicate reading detected within 15-minute window (matches reading {})",
                existing_id
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
        let query = if let Some(_minted) = minted_filter {
            format!(
                r#"
                SELECT
                    id, user_id, wallet_address,
                    kwh_amount, reading_timestamp, submitted_at,
                    minted, mint_tx_signature, meter_id, verification_status,
                    meter_serial
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
                    minted, mint_tx_signature, meter_id, verification_status,
                    meter_serial
                FROM meter_readings
                WHERE user_id = $1
                ORDER BY {} {}
                LIMIT $2 OFFSET $3
                "#,
                sort_by, sort_order
            )
        };

        let rows = if let Some(minted) = minted_filter {
            sqlx::query(&query)
                .bind(user_id)
                .bind(minted)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user readings: {}", e))?
        } else {
            sqlx::query(&query)
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user readings: {}", e))?
        };

        let readings: Result<Vec<MeterReading>, anyhow::Error> = rows
            .into_iter()
            .map(|row| {
                Ok(MeterReading {
                    id: row.get("id"),
                    user_id: row.get("user_id"),
                    wallet_address: row.get("wallet_address"),
                    kwh_amount: row.get("kwh_amount"),
                    reading_timestamp: row.get("reading_timestamp"),
                    submitted_at: row.get("submitted_at"),
                    minted: row.get("minted"),
                    mint_tx_signature: row.get("mint_tx_signature"),
                    meter_id: row.get("meter_id"),
                    meter_serial: row.get("meter_serial"),
                    verification_status: row.get("verification_status"),
                })
            })
            .collect();

        let readings = readings.map_err(|e| anyhow!("Failed to parse user readings: {}", e))?;

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
                "SELECT COUNT(*) FROM meter_readings WHERE user_id = $1 AND minted = $2",
            )
            .bind(user_id)
            .bind(minted)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user readings: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM meter_readings WHERE user_id = $1")
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
        let query = if let Some(_minted) = minted_filter {
            format!(
                r#"
                SELECT
                    id, user_id, wallet_address,
                    kwh_amount, reading_timestamp, submitted_at,
                    minted, mint_tx_signature, meter_id, verification_status,
                    meter_serial
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
                    minted, mint_tx_signature, meter_id, verification_status,
                    meter_serial
                FROM meter_readings
                WHERE wallet_address = $1
                ORDER BY {} {}
                LIMIT $2 OFFSET $3
                "#,
                sort_by, sort_order
            )
        };

        let rows = if let Some(minted) = minted_filter {
            sqlx::query(&query)
                .bind(wallet_address)
                .bind(minted)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch wallet readings: {}", e))?
        } else {
            sqlx::query(&query)
                .bind(wallet_address)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch wallet readings: {}", e))?
        };

        let readings: Result<Vec<MeterReading>, anyhow::Error> = rows
            .into_iter()
            .map(|row| {
                Ok(MeterReading {
                    id: row.get("id"),
                    user_id: row.get("user_id"),
                    wallet_address: row.get("wallet_address"),
                    kwh_amount: row.get("kwh_amount"),
                    reading_timestamp: row.get("reading_timestamp"),
                    submitted_at: row.get("submitted_at"),
                    minted: row.get("minted"),
                    mint_tx_signature: row.get("mint_tx_signature"),
                    meter_id: row.get("meter_id"),
                    meter_serial: row.get("meter_serial"),
                    verification_status: row.get("verification_status"),
                })
            })
            .collect();

        let readings = readings.map_err(|e| anyhow!("Failed to parse wallet readings: {}", e))?;

        debug!(
            "Retrieved {} readings for wallet {}",
            readings.len(),
            wallet_address
        );

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
                "SELECT COUNT(*) FROM meter_readings WHERE wallet_address = $1 AND minted = $2",
            )
            .bind(wallet_address)
            .bind(minted)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count wallet readings: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM meter_readings WHERE wallet_address = $1",
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
        let rows = sqlx::query(
            r#"
            SELECT
                id, user_id, wallet_address,
                kwh_amount, reading_timestamp, submitted_at,
                minted, mint_tx_signature, meter_id, verification_status,
                meter_serial
            FROM meter_readings
            WHERE minted = false
            ORDER BY submitted_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch unminted readings: {}", e))?;

        let readings: Result<Vec<MeterReading>, anyhow::Error> = rows
            .into_iter()
            .map(|row| {
                Ok(MeterReading {
                    id: row.get("id"),
                    user_id: row.get("user_id"),
                    wallet_address: row.get("wallet_address"),
                    kwh_amount: row.get("kwh_amount"),
                    reading_timestamp: row.get("reading_timestamp"),
                    submitted_at: row.get("submitted_at"),
                    minted: row.get("minted"),
                    mint_tx_signature: row.get("mint_tx_signature"),
                    meter_id: row.get("meter_id"),
                    meter_serial: row.get("meter_serial"),
                    verification_status: row.get("verification_status"),
                })
            })
            .collect();

        let readings = readings.map_err(|e| anyhow!("Failed to parse unminted readings: {}", e))?;
        debug!("Retrieved {} unminted readings", readings.len());

        Ok(readings)
    }

    /// Mark a reading as minted
    pub async fn mark_as_minted(
        &self,
        reading_id: Uuid,
        tx_signature: &str,
    ) -> Result<MeterReading> {
        let row = sqlx::query(
            r#"
            UPDATE meter_readings
            SET minted = true, mint_tx_signature = $2
            WHERE id = $1
            RETURNING
                id, user_id, wallet_address,
                kwh_amount, reading_timestamp, submitted_at,
                minted, mint_tx_signature, meter_id, verification_status,
                meter_serial
            "#,
        )
        .bind(reading_id)
        .bind(tx_signature)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to mark reading as minted: {}", e))?;

        let reading = MeterReading {
            id: row.get("id"),
            user_id: row.get("user_id"),
            wallet_address: row.get("wallet_address"),
            kwh_amount: row.get("kwh_amount"),
            reading_timestamp: row.get("reading_timestamp"),
            submitted_at: row.get("submitted_at"),
            minted: row.get("minted"),
            mint_tx_signature: row.get("mint_tx_signature"),
            meter_id: row.get("meter_id"),
            meter_serial: row.get("meter_serial"),
            verification_status: row.get("verification_status"),
        };

        info!(
            "Marked reading {} as minted with tx: {}",
            reading_id, tx_signature
        );

        Ok(reading)
    }

    /// Get a specific reading by ID
    pub async fn get_reading_by_id(&self, reading_id: Uuid) -> Result<MeterReading> {
        let row = sqlx::query(
            r#"
            SELECT
                id, user_id, wallet_address,
                kwh_amount, reading_timestamp, submitted_at,
                minted, mint_tx_signature, meter_id, verification_status,
                meter_serial
            FROM meter_readings
            WHERE id = $1
            "#,
        )
        .bind(reading_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch reading: {}", e))?;

        let reading = match row {
            Some(row) => MeterReading {
                id: row.get("id"),
                user_id: row.get("user_id"),
                wallet_address: row.get("wallet_address"),
                kwh_amount: row.get("kwh_amount"),
                reading_timestamp: row.get("reading_timestamp"),
                submitted_at: row.get("submitted_at"),
                minted: row.get("minted"),
                mint_tx_signature: row.get("mint_tx_signature"),
                meter_id: row.get("meter_id"),
                meter_serial: row.get("meter_serial"),
                verification_status: row.get("verification_status"),
            },
            None => return Err(anyhow!("Reading not found")),
        };

        Ok(reading)
    }

    /// Calculate total unminted kWh for a user
    pub async fn get_unminted_total(&self, user_id: Uuid) -> Result<Decimal> {
        let total: Decimal = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(kwh_amount), 0) FROM meter_readings WHERE user_id = $1 AND minted = false"#,
        )
        .bind(user_id)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to calculate unminted total: {}", e))?;

        Ok(total)
    }

    /// Get total minted kWh for a user
    pub async fn get_minted_total(&self, user_id: Uuid) -> Result<Decimal> {
        let total: Decimal = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(kwh_amount), 0) FROM meter_readings WHERE user_id = $1 AND minted = true"#,
        )
        .bind(user_id)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to calculate minted total: {}", e))?;

        Ok(total)
    }

    /// Validate meter reading data
    pub fn validate_reading(request: &SubmitMeterReadingRequest) -> Result<()> {
        // Amount validation
        // Allow negative amounts for consumption (burn)
        // if request.kwh_amount <= Decimal::ZERO {
        //     return Err(anyhow!("kWh amount must be positive"));
        // }

        // Maximum reasonable amount (e.g., 100 kWh per reading)
        let max_kwh = Decimal::from(100);
        if request.kwh_amount > max_kwh {
            warn!("Unusually high kWh reading: {}", request.kwh_amount);
            return Err(anyhow!("kWh amount exceeds maximum ({} kWh)", max_kwh));
        }

        // Timestamp validation
        // Allow 5 minutes of clock skew into the future
        if request.reading_timestamp > Utc::now() + chrono::Duration::minutes(5) {
            return Err(anyhow!(
                "Reading timestamp cannot be in the future (beyond 5m tolerance)"
            ));
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
            kwh_amount: Decimal::from(10),
            reading_timestamp: Utc::now(),
            meter_signature: None,
            meter_serial: None,
        };

        assert!(MeterService::validate_reading(&request).is_ok());
    }

    #[test]
    fn test_validate_reading_zero_amount() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: Decimal::from(0),
            reading_timestamp: Utc::now(),
            meter_signature: None,
            meter_serial: None,
        };

        // Code allows 0 amount (commented out check), so valid
        assert!(MeterService::validate_reading(&request).is_ok());
    }

    #[test]
    fn test_validate_reading_future_timestamp_tolerance() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: Decimal::from(10),
            reading_timestamp: Utc::now() + chrono::Duration::minutes(4), // Within 5m tolerance
            meter_signature: None,
            meter_serial: None,
        };

        assert!(MeterService::validate_reading(&request).is_ok());
    }

    #[test]
    fn test_validate_reading_future_timestamp() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: Decimal::from(10),
            reading_timestamp: Utc::now() + chrono::Duration::hours(1),
            meter_signature: None,
            meter_serial: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }

    #[test]
    fn test_validate_reading_excessive_amount() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: Decimal::from(150), // Over 100 kWh limit
            reading_timestamp: Utc::now(),
            meter_signature: None,
            meter_serial: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }

    #[test]
    fn test_validate_reading_old_timestamp() {
        let request = SubmitMeterReadingRequest {
            wallet_address: "test".to_string(),
            kwh_amount: Decimal::from(10),
            reading_timestamp: Utc::now() - chrono::Duration::days(10),
            meter_signature: None,
            meter_serial: None,
        };

        assert!(MeterService::validate_reading(&request).is_err());
    }
}
