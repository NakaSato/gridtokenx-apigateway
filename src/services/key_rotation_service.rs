use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::Instant;
use tracing::{error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

/// Service for managing encryption key rotation
pub struct KeyRotationService {
    db: PgPool,
}

/// Report of a key rotation operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotationReport {
    pub total_users: usize,
    pub successful: usize,
    pub failed: usize,
    pub duration_seconds: f64,
    pub errors: Vec<String>,
    pub new_version: i32,
}

/// Current status of key rotation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotationStatus {
    pub current_version: i32,
    pub total_keys: i32,
    pub active_key_version: i32,
    pub users_by_version: Vec<(i32, i64)>,
}

impl KeyRotationService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Rotate encryption keys for all users
    /// This is an atomic operation that re-encrypts all wallets
    pub async fn rotate_all_keys(
        &self,
        old_secret: &str,
        new_secret: &str,
        new_version: i32,
    ) -> Result<RotationReport> {
        let start_time = Instant::now();
        info!("Starting key rotation to version {}", new_version);

        // Validate new secret
        if new_secret.len() < 32 {
            return Err(anyhow!("New secret must be at least 32 characters"));
        }

        // Calculate hash of new key for verification
        let new_key_hash = Self::hash_key(new_secret);

        // Begin transaction
        let mut tx = self.db.begin().await?;

        // Create new key version entry
        sqlx::query!(
            "INSERT INTO encryption_keys (version, key_hash, notes)
             VALUES ($1, $2, $3)",
            new_version,
            new_key_hash,
            format!("Key rotation initiated at {}", Utc::now())
        )
        .execute(&mut *tx)
        .await?;

        // Get all users with encrypted wallets
        let users = sqlx::query!(
            "SELECT id, encrypted_private_key, wallet_salt, encryption_iv, key_version
             FROM users
             WHERE encrypted_private_key IS NOT NULL"
        )
        .fetch_all(&mut *tx)
        .await?;

        let total_users = users.len();
        let mut successful = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        info!("Rotating keys for {} users", total_users);

        // Rotate each user's key
        for user in users {
            match self
                .rotate_single_user(
                    &mut tx,
                    user.id,
                    &user.encrypted_private_key.unwrap(),
                    &user.wallet_salt.unwrap(),
                    &user.encryption_iv.unwrap(),
                    old_secret,
                    new_secret,
                    new_version,
                )
                .await
            {
                Ok(_) => {
                    successful += 1;
                    if successful % 100 == 0 {
                        info!("Progress: {}/{} users rotated", successful, total_users);
                    }
                }
                Err(e) => {
                    failed += 1;
                    let error_msg = format!("User {}: {}", user.id, e);
                    warn!("{}", error_msg);
                    errors.push(error_msg);

                    // If too many failures, abort
                    if failed > total_users / 10 {
                        // More than 10% failure rate
                        error!(
                            "Too many failures ({}/{}), aborting rotation",
                            failed, total_users
                        );
                        return Err(anyhow!("Rotation aborted due to high failure rate"));
                    }
                }
            }
        }

        // Mark old key as inactive
        sqlx::query!(
            "UPDATE encryption_keys SET is_active = false, rotated_at = NOW()
             WHERE version < $1",
            new_version
        )
        .execute(&mut *tx)
        .await?;

        // Commit transaction
        tx.commit().await?;

        let duration = start_time.elapsed().as_secs_f64();
        info!(
            "Key rotation completed: {}/{} successful in {:.2}s",
            successful, total_users, duration
        );

        Ok(RotationReport {
            total_users,
            successful,
            failed,
            duration_seconds: duration,
            errors,
            new_version,
        })
    }

    /// Rotate a single user's encryption key
    async fn rotate_single_user(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        encrypted_key: &[u8],
        salt: &[u8],
        iv: &[u8],
        old_secret: &str,
        new_secret: &str,
        new_version: i32,
    ) -> Result<()> {
        // Decrypt with old key
        let decrypted_bytes =
            crate::utils::crypto::decrypt_bytes(encrypted_key, salt, iv, old_secret)?;

        // Re-encrypt with new key
        let (new_encrypted, new_salt, new_iv) =
            crate::utils::crypto::encrypt_to_bytes(&decrypted_bytes, new_secret)?;

        // Update user's wallet with new encryption
        sqlx::query!(
            "UPDATE users 
             SET encrypted_private_key = $1,
                 wallet_salt = $2,
                 encryption_iv = $3,
                 key_version = $4
             WHERE id = $5",
            &new_encrypted[..],
            &new_salt[..],
            &new_iv[..],
            new_version,
            user_id
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Get current rotation status
    pub async fn get_rotation_status(&self) -> Result<RotationStatus> {
        // Get active key version
        let active_key = sqlx::query!(
            "SELECT version FROM encryption_keys WHERE is_active = true ORDER BY version DESC LIMIT 1"
        )
        .fetch_optional(&self.db)
        .await?;

        let active_key_version = active_key.map(|k| k.version).unwrap_or(1);

        // Get total number of key versions
        let total_keys = sqlx::query!("SELECT COUNT(*) as count FROM encryption_keys")
            .fetch_one(&self.db)
            .await?
            .count
            .unwrap_or(0) as i32;

        // Get distribution of users by key version
        let users_by_version = sqlx::query!(
            "SELECT key_version, COUNT(*) as count 
             FROM users 
             WHERE encrypted_private_key IS NOT NULL
             GROUP BY key_version
             ORDER BY key_version"
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|row| (row.key_version.unwrap_or(1), row.count.unwrap_or(0)))
        .collect();

        Ok(RotationStatus {
            current_version: active_key_version,
            total_keys,
            active_key_version,
            users_by_version,
        })
    }

    /// Rollback to a previous key version
    pub async fn rollback_rotation(
        &self,
        target_version: i32,
        current_secret: &str,
        target_secret: &str,
    ) -> Result<RotationReport> {
        info!("Rolling back to key version {}", target_version);

        // Verify target version exists
        let target_key = sqlx::query!(
            "SELECT version FROM encryption_keys WHERE version = $1",
            target_version
        )
        .fetch_optional(&self.db)
        .await?;

        if target_key.is_none() {
            return Err(anyhow!("Target key version {} not found", target_version));
        }

        // Perform rotation back to target version
        self.rotate_all_keys(current_secret, target_secret, target_version)
            .await
    }

    /// Calculate SHA-256 hash of a key for verification
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
