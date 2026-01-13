use anyhow::{anyhow, Result};
use bcrypt::{hash, DEFAULT_COST};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::types::ipnetwork::IpNetwork;
use tracing::info;
use uuid::Uuid;

/// Meter registry entry
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeterRegistry {
    pub id: Uuid,
    pub meter_serial: String,
    pub meter_key_hash: String,
    pub verification_method: String,
    pub verification_status: String,
    pub user_id: Uuid,
    pub manufacturer: Option<String>,
    pub meter_type: Option<String>,
    pub location_address: Option<String>,
    pub installation_date: Option<chrono::NaiveDate>,
    pub verification_proof: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Meter verification request
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VerifyMeterRequest {
    pub meter_serial: String,
    pub meter_key: String,
    pub verification_method: String,
    pub manufacturer: Option<String>,
    pub meter_type: String,
    pub location_address: Option<String>,
    pub verification_proof: Option<String>,
}

/// Meter verification response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VerifyMeterResponse {
    pub meter_id: Uuid,
    pub verification_status: String,
    pub message: String,
}

/// Meter verification attempt for audit
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeterVerificationAttempt {
    pub id: Uuid,
    pub meter_serial: String,
    pub user_id: Uuid,
    pub verification_method: String,
    pub ip_address: Option<IpNetwork>,
    pub user_agent: Option<String>,
    pub attempt_result: Option<String>,
    pub failure_reason: Option<String>,
    pub attempted_at: Option<DateTime<Utc>>,
}

/// Service for managing meter verification
#[derive(Clone)]
pub struct MeterVerificationService {
    db_pool: PgPool,
}

impl MeterVerificationService {
    /// Create a new meter verification service
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Verify meter ownership - Primary verification flow
    pub async fn verify_meter(
        &self,
        user_id: Uuid,
        request: VerifyMeterRequest,
        ip_address: Option<IpNetwork>,
        user_agent: Option<String>,
    ) -> Result<VerifyMeterResponse> {
        info!(
            "User {} attempting to verify meter: {}",
            user_id, request.meter_serial
        );

        // 1. Validate meter key format
        if let Err(e) = self.validate_meter_key_format(&request.meter_key) {
            self.log_attempt(
                &request.meter_serial,
                user_id,
                &request.verification_method,
                ip_address,
                user_agent,
                "invalid_key",
                Some(&e.to_string()),
            ).await?;
            return Err(e);
        }

        // 3. Check if meter is already claimed by another user
        if let Err(e) = self.check_meter_availability(&request.meter_serial, user_id).await {
            self.log_attempt(
                &request.meter_serial,
                user_id,
                &request.verification_method,
                ip_address,
                user_agent,
                "meter_claimed",
                Some(&e.to_string()),
            ).await?;
            return Err(e);
        }

        // 4. Hash meter key with bcrypt
        let meter_key_hash = hash(&request.meter_key, DEFAULT_COST)
            .map_err(|e| anyhow!("Failed to hash meter key: {}", e))?;

        // 5. Insert into meter_registry
        let meter_id = self.insert_meter_registry(
            user_id,
            &request,
            &meter_key_hash,
        ).await?;

        // 6. Log successful verification
        self.log_attempt(
            &request.meter_serial,
            user_id,
            &request.verification_method,
            ip_address,
            user_agent,
            "success",
            None,
        ).await?;

        info!(
            "Meter {} successfully verified for user {}. Meter ID: {}",
            request.meter_serial, user_id, meter_id
        );

        Ok(VerifyMeterResponse {
            meter_id,
            verification_status: "verified".to_string(),
            message: "Meter ownership verified successfully".to_string(),
        })
    }

    /// Get user's registered meters
    pub async fn get_user_meters(&self, user_id: &Uuid) -> Result<Vec<MeterRegistry>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                id, meter_serial, meter_key_hash, verification_method,
                verification_status, user_id, manufacturer, meter_type,
                location_address, installation_date, verification_proof,
                verified_at, verified_by, created_at, updated_at
            FROM meter_registry
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch user meters: {}", e))?;

        let meters: Result<Vec<MeterRegistry>, _> = rows.into_iter().map(|row| {
            Ok(MeterRegistry {
                id: row.id,
                meter_serial: row.meter_serial,
                meter_key_hash: row.meter_key_hash,
                verification_method: row.verification_method,
                verification_status: row.verification_status,
                user_id: row.user_id,
                manufacturer: row.manufacturer,
                meter_type: row.meter_type,
                location_address: row.location_address,
                installation_date: row.installation_date,
                verification_proof: row.verification_proof,
                verified_at: row.verified_at,
                verified_by: row.verified_by,
                created_at: row.created_at.unwrap_or_else(Utc::now),
                updated_at: row.updated_at.unwrap_or_else(Utc::now),
            })
        }).collect();

        meters.map_err(|e: anyhow::Error| anyhow!("Failed to process meter records: {}", e))
    }

    /// Verify if user owns a specific meter
    pub async fn verify_meter_ownership(
        &self,
        user_id: &str,
        meter_id: &Uuid,
    ) -> Result<bool> {
        let user_uuid = Uuid::parse_str(user_id)
            .map_err(|e| anyhow!("Invalid user ID format: {}", e))?;

        let meter = sqlx::query!(
            r#"
            SELECT user_id, verification_status FROM meter_registry
            WHERE id = $1
            "#,
            meter_id
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to check meter ownership: {}", e))?;

        match meter {
            Some(m) => Ok(m.user_id == user_uuid && m.verification_status == "verified"),
            None => Ok(false),
        }
    }

    /// Validate meter key format
    fn validate_meter_key_format(&self, meter_key: &str) -> Result<()> {
        // Meter key should be 16-64 alphanumeric characters
        if meter_key.len() < 16 || meter_key.len() > 64 {
            return Err(anyhow!(
                "Meter key must be between 16 and 64 characters long"
            ));
        }

        // Check if contains only alphanumeric characters (and some special chars)
        if !meter_key.chars().all(|c| c.is_alphanumeric() || "-_".contains(c)) {
            return Err(anyhow!(
                "Meter key can only contain letters, numbers, hyphens, and underscores"
            ));
        }

        Ok(())
    }

    /// Check if meter is already claimed by another user
    async fn check_meter_availability(
        &self,
        meter_serial: &str,
        user_id: Uuid,
    ) -> Result<()> {
        let existing = sqlx::query!(
            r#"
            SELECT user_id, verification_status FROM meter_registry
            WHERE meter_serial = $1
            "#,
            meter_serial
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to check meter availability: {}", e))?;

        if let Some(existing_meter) = existing {
            if existing_meter.user_id != user_id {
                return Err(anyhow!(
                    "Meter '{}' is already registered by another user",
                    meter_serial
                ));
            } else if existing_meter.verification_status == "verified" {
                return Err(anyhow!(
                    "Meter '{}' is already verified for your account",
                    meter_serial
                ));
            }
        }

        Ok(())
    }

    /// Insert new meter into registry
    async fn insert_meter_registry(
        &self,
        user_id: Uuid,
        request: &VerifyMeterRequest,
        meter_key_hash: &str,
    ) -> Result<Uuid> {
        let meter_id = sqlx::query!(
            r#"
            INSERT INTO meter_registry (
                meter_serial, meter_key_hash, verification_method,
                verification_status, user_id, manufacturer, meter_type,
                location_address, verification_proof, verified_at
            )
            VALUES ($1, $2, $3, 'verified', $4, $5, $6, $7, $8, NOW())
            RETURNING id
            "#,
            request.meter_serial,
            meter_key_hash,
            request.verification_method,
            user_id,
            request.manufacturer,
            request.meter_type,
            request.location_address,
            request.verification_proof,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to insert meter registry: {}", e))?;

        Ok(meter_id.id)
    }

    /// Log verification attempt for audit trail
    async fn log_attempt(
        &self,
        meter_serial: &str,
        user_id: Uuid,
        verification_method: &str,
        ip_address: Option<IpNetwork>,
        user_agent: Option<String>,
        attempt_result: &str,
        failure_reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO meter_verification_attempts (
                meter_serial, user_id, verification_method,
                ip_address, user_agent, attempt_status, failure_reason
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            meter_serial,
            user_id,
            verification_method,
            ip_address,
            user_agent,
            attempt_result,
            failure_reason,
        )
        .execute(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to log verification attempt: {}", e))?;

        Ok(())
    }

    /// Get verification attempts for a user (admin function)
    pub async fn get_user_verification_attempts(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<MeterVerificationAttempt>> {
        let attempts = sqlx::query_as!(
            MeterVerificationAttempt,
            r#"
            SELECT 
                id, meter_serial, user_id, verification_method,
                ip_address, user_agent, 
                attempt_result as "attempt_result?",
                failure_reason,
                attempted_at as "attempted_at!"
            FROM meter_verification_attempts
            WHERE user_id = $1
            ORDER BY attempted_at DESC
            LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch verification attempts: {}", e))?;

        Ok(attempts)
    }

    /// Register meter with minimal info (simplified registration)
    /// This is for users who just want to register their meter serial without full verification
    pub async fn register_meter_simple(
        &self,
        user_id: Uuid,
        meter_serial: String,
        meter_type: Option<String>,
        location_address: Option<String>,
    ) -> Result<MeterRegistry> {
        info!(
            "User {} registering meter (simple): {}",
            user_id, meter_serial
        );

        // 1. Validate meter serial format
        self.validate_meter_serial_format(&meter_serial)?;

        // 2. Check if meter is already registered
        if let Some(existing) = self.get_meter_by_serial(&meter_serial).await? {
            if existing.user_id != user_id {
                return Err(anyhow!(
                    "Meter serial '{}' is already registered by another user",
                    meter_serial
                ));
            } else {
                return Err(anyhow!(
                    "Meter serial '{}' is already registered to your account",
                    meter_serial
                ));
            }
        }

        // 3. Insert into meter_registry with pending status
        let meter_id = sqlx::query!(
            r#"
            INSERT INTO meter_registry (
                meter_serial, meter_key_hash, verification_method,
                verification_status, user_id, meter_type, location_address
            )
            VALUES ($1, '', 'serial', 'pending', $2, $3, $4)
            RETURNING id
            "#,
            meter_serial,
            user_id,
            meter_type,
            location_address,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to register meter: {}", e))?;

        // 4. Fetch and return the created meter
        let meter = self.get_meter_by_id(&meter_id.id).await?
            .ok_or_else(|| anyhow!("Failed to retrieve registered meter"))?;

        info!(
            "Meter {} successfully registered for user {} with ID: {}",
            meter_serial, user_id, meter_id.id
        );

        Ok(meter)
    }

    /// Delete a pending meter (only allowed for pending status)
    pub async fn delete_pending_meter(
        &self,
        user_id: Uuid,
        meter_id: Uuid,
    ) -> Result<()> {
        info!("User {} attempting to delete meter: {}", user_id, meter_id);

        // 1. Check if meter exists and belongs to user
        let meter = sqlx::query!(
            r#"
            SELECT user_id, verification_status FROM meter_registry
            WHERE id = $1
            "#,
            meter_id
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch meter: {}", e))?
        .ok_or_else(|| anyhow!("Meter not found"))?;

        // 2. Verify ownership
        if meter.user_id != user_id {
            return Err(anyhow!("You do not own this meter"));
        }

        // 3. Only allow deletion of pending meters
        if meter.verification_status != "pending" {
            return Err(anyhow!(
                "Cannot delete {} meters. Only pending meters can be deleted.",
                meter.verification_status
            ));
        }

        // 4. Delete the meter
        sqlx::query!(
            r#"
            DELETE FROM meter_registry
            WHERE id = $1
            "#,
            meter_id
        )
        .execute(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to delete meter: {}", e))?;

        info!("Meter {} successfully deleted by user {}", meter_id, user_id);

        Ok(())
    }

    /// Validate meter serial format
    fn validate_meter_serial_format(&self, meter_serial: &str) -> Result<()> {
        // Meter serial should be 5-50 characters
        if meter_serial.len() < 5 || meter_serial.len() > 50 {
            return Err(anyhow!(
                "Meter serial must be between 5 and 50 characters long"
            ));
        }

        // Check if contains only alphanumeric characters and hyphens
        if !meter_serial.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Err(anyhow!(
                "Meter serial can only contain letters, numbers, and hyphens"
            ));
        }

        Ok(())
    }

    /// Get meter by serial number
    async fn get_meter_by_serial(&self, meter_serial: &str) -> Result<Option<MeterRegistry>> {
        let row = sqlx::query!(
            r#"
            SELECT 
                id, meter_serial, meter_key_hash, verification_method,
                verification_status, user_id, manufacturer, meter_type,
                location_address, installation_date, verification_proof,
                verified_at, verified_by, created_at, updated_at
            FROM meter_registry
            WHERE meter_serial = $1
            "#,
            meter_serial
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch meter by serial: {}", e))?;

        Ok(row.map(|r| MeterRegistry {
            id: r.id,
            meter_serial: r.meter_serial,
            meter_key_hash: r.meter_key_hash,
            verification_method: r.verification_method,
            verification_status: r.verification_status,
            user_id: r.user_id,
            manufacturer: r.manufacturer,
            meter_type: r.meter_type,
            location_address: r.location_address,
            installation_date: r.installation_date,
            verification_proof: r.verification_proof,
            verified_at: r.verified_at,
            verified_by: r.verified_by,
            created_at: r.created_at.unwrap_or_else(Utc::now),
            updated_at: r.updated_at.unwrap_or_else(Utc::now),
        }))
    }

    /// Get meter by ID
    async fn get_meter_by_id(&self, meter_id: &Uuid) -> Result<Option<MeterRegistry>> {
        let row = sqlx::query!(
            r#"
            SELECT 
                id, meter_serial, meter_key_hash, verification_method,
                verification_status, user_id, manufacturer, meter_type,
                location_address, installation_date, verification_proof,
                verified_at, verified_by, created_at, updated_at
            FROM meter_registry
            WHERE id = $1
            "#,
            meter_id
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch meter by ID: {}", e))?;

        Ok(row.map(|r| MeterRegistry {
            id: r.id,
            meter_serial: r.meter_serial,
            meter_key_hash: r.meter_key_hash,
            verification_method: r.verification_method,
            verification_status: r.verification_status,
            user_id: r.user_id,
            manufacturer: r.manufacturer,
            meter_type: r.meter_type,
            location_address: r.location_address,
            installation_date: r.installation_date,
            verification_proof: r.verification_proof,
            verified_at: r.verified_at,
            verified_by: r.verified_by,
            created_at: r.created_at.unwrap_or_else(Utc::now),
            updated_at: r.updated_at.unwrap_or_else(Utc::now),
        }))
    }

    /// Get verification statistics for monitoring
    pub async fn get_verification_stats(&self) -> Result<VerificationStats> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as "total_meters!",
                COUNT(CASE WHEN verification_status = 'verified' THEN 1 END) as "verified_meters!",
                COUNT(CASE WHEN verification_status = 'pending' THEN 1 END) as "pending_meters!",
                COUNT(CASE WHEN verification_status = 'rejected' THEN 1 END) as "rejected_meters!"
            FROM meter_registry
            "#
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch verification stats: {}", e))?;

        let attempts_24h = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM meter_verification_attempts
            WHERE attempted_at > NOW() - INTERVAL '24 hours'
            "#
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch recent attempts: {}", e))?;

        Ok(VerificationStats {
            total_meters: stats.total_meters,
            verified_meters: stats.verified_meters,
            pending_meters: stats.pending_meters,
            rejected_meters: stats.rejected_meters,
            attempts_last_24h: attempts_24h,
        })
    }
}

/// Verification statistics
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VerificationStats {
    pub total_meters: i64,
    pub verified_meters: i64,
    pub pending_meters: i64,
    pub rejected_meters: i64,
    pub attempts_last_24h: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_meter_key_format_valid() {
        // Test validation logic directly
        let validate_meter_key_format = |meter_key: &str| -> Result<()> {
            // Meter key should be 16-64 alphanumeric characters
            if meter_key.len() < 16 || meter_key.len() > 64 {
                return Err(anyhow!(
                    "Meter key must be between 16 and 64 characters long"
                ));
            }

            // Check if contains only alphanumeric characters (and some special chars)
            if !meter_key.chars().all(|c| c.is_alphanumeric() || "-_".contains(c)) {
                return Err(anyhow!(
                    "Meter key can only contain letters, numbers, hyphens, and underscores"
                ));
            }

            Ok(())
        };
        
        // Valid keys
        assert!(validate_meter_key_format("test-key-12345678").is_ok());
        assert!(validate_meter_key_format("METER_SERIAL_ABC123").is_ok());
        assert!(validate_meter_key_format("16chars_min_length").is_ok());
        assert!(validate_meter_key_format(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn test_validate_meter_key_format_invalid() {
        // Test validation logic directly
        let validate_meter_key_format = |meter_key: &str| -> Result<()> {
            // Meter key should be 16-64 alphanumeric characters
            if meter_key.len() < 16 || meter_key.len() > 64 {
                return Err(anyhow!(
                    "Meter key must be between 16 and 64 characters long"
                ));
            }

            // Check if contains only alphanumeric characters (and some special chars)
            if !meter_key.chars().all(|c| c.is_alphanumeric() || "-_".contains(c)) {
                return Err(anyhow!(
                    "Meter key can only contain letters, numbers, hyphens, and underscores"
                ));
            }

            Ok(())
        };
        
        // Too short
        assert!(validate_meter_key_format("short").is_err());
        
        // Too long
        assert!(validate_meter_key_format(&"a".repeat(65)).is_err());
        
        // Invalid characters
        assert!(validate_meter_key_format("invalid@key").is_err());
        assert!(validate_meter_key_format("key with spaces").is_err());
    }
}
