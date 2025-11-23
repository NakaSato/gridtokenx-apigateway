use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::TokenizationConfig;
use crate::database::PgPool;
use crate::error::ApiError;
use crate::models::meter::{MeterReading, MeterReadingStatus};
use crate::services::blockchain_service::BlockchainService;
use crate::services::meter_service::MeterService;
use crate::services::websocket_service::WebSocketService;

/// Result of a minting operation
#[derive(Debug, Clone)]
pub struct MintResult {
    pub reading_id: Uuid,
    pub success: bool,
    pub error: Option<String>,
    pub tx_signature: Option<String>,
}

/// Automated polling service for meter readings
pub struct MeterPollingService {
    db: Arc<PgPool>,
    blockchain_service: Arc<BlockchainService>,
    meter_service: Arc<MeterService>,
    websocket_service: Arc<WebSocketService>,
    config: TokenizationConfig,
}

impl MeterPollingService {
    /// Create a new meter polling service
    pub fn new(
        db: Arc<PgPool>,
        blockchain_service: Arc<BlockchainService>,
        meter_service: Arc<MeterService>,
        websocket_service: Arc<WebSocketService>,
        config: TokenizationConfig,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            meter_service,
            websocket_service,
            config,
        }
    }

    /// Start the polling service
    pub async fn start(&self) {
        if !self.config.auto_mint_enabled {
            info!("Automated meter polling is disabled");
            return;
        }

        info!(
            "Starting meter polling service with interval: {}s",
            self.config.polling_interval_secs
        );

        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.process_unminted_readings().await {
                error!("Error processing unminted readings: {}", e);
            }
        }
    }

    /// Process all unminted readings
    async fn process_unminted_readings(&self) -> Result<(), ApiError> {
        debug!("Processing unminted readings");

        // Fetch unminted readings
        let readings = self
            .meter_service
            .get_unminted_readings(self.config.batch_size as i64)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to fetch unminted readings: {}", e)))?;

        if readings.is_empty() {
            debug!("No unminted readings found");
            return Ok(());
        }

        info!("Found {} unminted readings to process", readings.len());

        // Process readings in batches
        for batch in readings.chunks(self.config.max_transactions_per_batch) {
            let results = self.process_batch(batch.to_vec()).await?;

            // Count successful and failed mints
            let successful_count = results.iter().filter(|r| r.success).count();
            let failed_count = results.len() - successful_count;

            info!(
                "Batch processing completed: {} successful, {} failed",
                successful_count, failed_count
            );

            // Broadcast batch completion event
            let batch_id = Uuid::new_v4().to_string();
            if let Err(e) = self
                .websocket_service
                .broadcast_batch_minting_completed(
                    &batch_id,
                    batch.len() as u32,
                    successful_count as u32,
                    failed_count as u32,
                )
                .await
            {
                warn!("Failed to broadcast batch minting completed event: {}", e);
            }

            // Handle failed mints
            let failed_results: Vec<MintResult> =
                results.iter().filter(|r| !r.success).cloned().collect();

            if !failed_results.is_empty() {
                if let Err(e) = self.handle_failed_minting(&failed_results).await {
                    error!("Error handling failed minting: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Process a batch of readings
    async fn process_batch(
        &self,
        readings: Vec<MeterReading>,
    ) -> Result<Vec<MintResult>, ApiError> {
        let mut results = Vec::new();

        for reading in readings {
            // Validate reading
            match self.validate_reading(&reading) {
                Ok(_) => {
                    // Mint tokens for this reading
                    let result = self.mint_tokens_for_reading(&reading).await;
                    results.push(result);
                }
                Err(e) => {
                    warn!("Invalid reading {}: {:?}", reading.id, e);
                    results.push(MintResult {
                        reading_id: reading.id,
                        success: false,
                        error: Some(format!("Validation error: {:?}", e)),
                        tx_signature: None,
                    });

                    // Broadcast validation failure event
                    if let Err(broadcast_err) = self
                        .websocket_service
                        .broadcast_meter_reading_validation_failed(
                            &reading.user_id,
                            &reading.wallet_address,
                            &reading
                                .meter_serial
                                .unwrap_or_else(|| "unknown".to_string()),
                            &reading
                                .meter_serial
                                .as_deref()
                                .unwrap_or(&"unknown".to_string()),
                            reading.kwh_amount,
                            &format!("{:?}", e),
                        )
                        .await
                    {
                        warn!(
                            "Failed to broadcast validation failed event: {}",
                            broadcast_err
                        );
                    }
                }
            }
        }

        Ok(results)
    }

    /// Validate a meter reading
    fn validate_reading(
        &self,
        reading: &MeterReading,
    ) -> Result<(), crate::config::ValidationError> {
        // Check reading age
        let reading_age = chrono::Utc::now().signed_duration_since(reading.submitted_at);
        if reading_age.num_days() > self.config.reading_max_age_days {
            return Err(crate::config::ValidationError::ReadingTooOld);
        }

        // Check amount
        if reading.kwh_amount > self.config.max_reading_kwh {
            return Err(crate::config::ValidationError::AmountTooHigh(
                reading.kwh_amount,
            ));
        }

        // Check verification status
        if reading.verification_status != MeterReadingStatus::Verified {
            return Err(crate::config::ValidationError::InvalidConversion);
        }

        Ok(())
    }

    /// Mint tokens for a single reading
    async fn mint_tokens_for_reading(&self, reading: &MeterReading) -> MintResult {
        // Calculate tokens to mint
        let tokens_to_mint = match self.config.kwh_to_tokens(reading.kwh_amount) {
            Ok(amount) => amount,
            Err(e) => {
                return MintResult {
                    reading_id: reading.id,
                    success: false,
                    error: Some(format!("Token conversion error: {:?}", e)),
                    tx_signature: None,
                };
            }
        };

        // Mint tokens
        let wallet_pubkey = match reading.wallet_address.parse() {
            Ok(pubkey) => pubkey,
            Err(_) => {
                error!("Invalid wallet address: {}", reading.wallet_address);
                return MintResult {
                    reading_id: reading.id,
                    success: false,
                    error: Some("Invalid wallet address".to_string()),
                    tx_signature: None,
                };
            }
        };

        match self
            .blockchain_service
            .mint_tokens_direct(&wallet_pubkey, tokens_to_mint)
            .await
        {
            Ok(tx_signature) => {
                // Update database
                if let Err(e) = self
                    .meter_service
                    .mark_as_minted(&reading.id, &tx_signature)
                    .await
                {
                    error!("Failed to mark reading {} as minted: {}", reading.id, e);
                    return MintResult {
                        reading_id: reading.id,
                        success: false,
                        error: Some(format!("Database update error: {}", e)),
                        tx_signature: Some(tx_signature),
                    };
                }

                // Send WebSocket notification
                if let Err(e) = self
                    .websocket_service
                    .broadcast_tokens_minted(
                        &reading.user_id,
                        &reading.wallet_address,
                        &reading.meter_serial,
                        reading.kwh_amount,
                        tokens_to_mint,
                        &tx_signature,
                    )
                    .await
                {
                    warn!("Failed to broadcast tokens minted event: {}", e);
                }

                info!(
                    "Successfully minted {} tokens for reading {} (tx: {})",
                    tokens_to_mint, reading.id, tx_signature
                );

                MintResult {
                    reading_id: reading.id,
                    success: true,
                    error: None,
                    tx_signature: Some(tx_signature),
                }
            }
            Err(e) => {
                error!("Failed to mint tokens for reading {}: {}", reading.id, e);
                MintResult {
                    reading_id: reading.id,
                    success: false,
                    error: Some(format!("Minting error: {}", e)),
                    tx_signature: None,
                }
            }
        }
    }

    /// Handle failed minting operations
    async fn handle_failed_minting(&self, failed_results: &[MintResult]) -> Result<(), ApiError> {
        for result in failed_results {
            // Add to retry queue
            if let Err(e) = self.add_to_retry_queue(result).await {
                error!(
                    "Failed to add reading {} to retry queue: {}",
                    result.reading_id, e
                );
            }
        }
        Ok(())
    }

    /// Add a failed reading to the retry queue
    async fn add_to_retry_queue(&self, result: &MintResult) -> Result<(), ApiError> {
        // Check if already in retry queue
        let existing = sqlx::query!(
            "SELECT id FROM minting_retry_queue WHERE reading_id = $1",
            result.reading_id
        )
        .fetch_optional(self.db.as_ref())
        .await?;

        if existing.is_some() {
            // Update existing entry
            sqlx::query!(
                r#"
                UPDATE minting_retry_queue
                SET attempts = attempts + 1,
                    error_message = $2,
                    next_retry_at = CASE
                        WHEN attempts >= $3 THEN NOW() + INTERVAL '1 hour'
                        ELSE NOW() + INTERVAL '5 minutes' * POWER($4, attempts)
                    END,
                    updated_at = NOW()
                WHERE reading_id = $1
                "#,
                result.reading_id,
                result
                    .error
                    .as_ref()
                    .unwrap_or(&"Unknown error".to_string()),
                self.config.max_retry_attempts as i64,
                self.config.retry_backoff_multiplier
            )
            .execute(self.db.as_ref())
            .await?;
        } else {
            // Insert new entry
            sqlx::query!(
                r#"
                INSERT INTO minting_retry_queue
                (reading_id, error_message, attempts, next_retry_at, created_at, updated_at)
                VALUES ($1, $2, 1, NOW() + INTERVAL '5 minutes', NOW(), NOW())
                "#,
                result.reading_id,
                result
                    .error
                    .as_ref()
                    .unwrap_or(&"Unknown error".to_string()),
            )
            .execute(self.db.as_ref())
            .await?;
        }

        Ok(())
    }

    /// Process readings from the retry queue
    pub async fn process_retry_queue(&self) -> Result<(), ApiError> {
        debug!("Processing retry queue");

        // Fetch readings that are due for retry
        let retry_readings = sqlx::query!(
            r#"
            SELECT mr.id, mr.user_id, mr.wallet_address, mr.kwh_amount,
                   mr.reading_timestamp, mr.submitted_at, mr.meter_serial,
                   mrq.attempts
            FROM meter_readings mr
            JOIN minting_retry_queue mrq ON mr.id = mrq.reading_id
            WHERE mrq.next_retry_at <= NOW()
            AND mrq.attempts < $1
            ORDER BY mrq.next_retry_at ASC
            LIMIT $2
            "#,
            self.config.max_retry_attempts as i64,
            self.config.batch_size as i64
        )
        .fetch_all(self.db.as_ref())
        .await?;

        if retry_readings.is_empty() {
            debug!("No readings in retry queue ready for processing");
            return Ok(());
        }

        info!(
            "Found {} readings in retry queue to process",
            retry_readings.len()
        );

        // Convert to MeterReading objects
        let readings: Vec<MeterReading> = retry_readings
            .into_iter()
            .map(|row| MeterReading {
                id: row.id,
                user_id: row.user_id,
                wallet_address: row.wallet_address,
                kwh_amount: row.kwh_amount,
                reading_timestamp: row.reading_timestamp,
                submitted_at: row.submitted_at,
                minted: false,
                mint_tx_signature: None,
                meter_serial: row.meter_serial,
                verification_status: MeterReadingStatus::Verified, // Assuming verified if in retry queue
            })
            .collect();

        // Process the batch
        let results = self.process_batch(readings).await?;

        // Remove successful mints from retry queue
        for result in &results {
            if result.success {
                sqlx::query!(
                    "DELETE FROM minting_retry_queue WHERE reading_id = $1",
                    result.reading_id
                )
                .execute(self.db.as_ref())
                .await?;
            }
        }

        // Mark readings that exceeded max retries as failed
        sqlx::query!(
            r#"
            UPDATE minting_retry_queue mrq
            SET error_message = error_message || ' - Max retry attempts exceeded'
            FROM meter_readings mr
            WHERE mrq.reading_id = mr.id
            AND mrq.attempts >= $1
            "#,
            self.config.max_retry_attempts as i64
        )
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TokenizationConfig;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_meter_reading() -> MeterReading {
        MeterReading {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            wallet_address: "test_wallet_address".to_string(),
            kwh_amount: 10.0,
            reading_timestamp: Utc::now(),
            submitted_at: Utc::now(),
            minted: false,
            mint_tx_signature: None,
            meter_serial: "test_meter_001".to_string(),
            verification_status: MeterReadingStatus::Verified,
        }
    }

    fn create_test_config() -> TokenizationConfig {
        TokenizationConfig {
            kwh_to_token_ratio: 1.0,
            decimals: 9,
            max_reading_kwh: 100.0,
            reading_max_age_days: 7,
            auto_mint_enabled: true,
            polling_interval_secs: 60,
            batch_size: 50,
            max_retry_attempts: 3,
            initial_retry_delay_secs: 300,
            retry_backoff_multiplier: 2.0,
            max_retry_delay_secs: 3600,
            transaction_timeout_secs: 60,
            max_transactions_per_batch: 20,
        }
    }

    #[test]
    fn test_validate_reading_valid() {
        let config = create_test_config();
        let service = MeterPollingService {
            db: Arc::new(PgPool::connect("sqlite::memory:").await.unwrap()),
            blockchain_service: Arc::new(
                BlockchainService::new("test_url".to_string(), "test".to_string()).unwrap(),
            ),
            meter_service: Arc::new(MeterService::new(Arc::new(
                PgPool::connect("sqlite::memory:").await.unwrap(),
            ))),
            websocket_service: Arc::new(WebSocketService::new()),
            config,
        };

        let mut reading = create_test_meter_reading();
        reading.submitted_at = Utc::now();
        reading.kwh_amount = 50.0;
        reading.verification_status = MeterReadingStatus::Verified;

        // This should not return an error
        assert!(service.validate_reading(&reading).is_ok());
    }

    #[test]
    fn test_validate_reading_too_old() {
        let config = create_test_config();
        let service = MeterPollingService {
            db: Arc::new(PgPool::connect("sqlite::memory:").await.unwrap()),
            blockchain_service: Arc::new(
                BlockchainService::new("test_url".to_string(), "test".to_string()).unwrap(),
            ),
            meter_service: Arc::new(MeterService::new(Arc::new(
                PgPool::connect("sqlite::memory:").await.unwrap(),
            ))),
            websocket_service: Arc::new(WebSocketService::new()),
            config,
        };

        let mut reading = create_test_meter_reading();
        reading.submitted_at = Utc::now() - chrono::Duration::days(10); // 10 days ago

        // This should return an error because the reading is too old
        match service.validate_reading(&reading) {
            Err(crate::config::ValidationError::ReadingTooOld) => {
                // Expected error
            }
            _ => panic!("Expected ValidationError::ReadingTooOld"),
        }
    }

    #[test]
    fn test_validate_reading_amount_too_high() {
        let config = create_test_config();
        let service = MeterPollingService {
            db: Arc::new(PgPool::connect("sqlite::memory:").await.unwrap()),
            blockchain_service: Arc::new(
                BlockchainService::new("test_url".to_string(), "test".to_string()).unwrap(),
            ),
            meter_service: Arc::new(MeterService::new(Arc::new(
                PgPool::connect("sqlite::memory:").await.unwrap(),
            ))),
            websocket_service: Arc::new(WebSocketService::new()),
            config,
        };

        let mut reading = create_test_meter_reading();
        reading.kwh_amount = 500.0; // Exceeds max_reading_kwh

        // This should return an error because the amount is too high
        match service.validate_reading(&reading) {
            Err(crate::config::ValidationError::AmountTooHigh(_)) => {
                // Expected error
            }
            _ => panic!("Expected ValidationError::AmountTooHigh"),
        }
    }
}
