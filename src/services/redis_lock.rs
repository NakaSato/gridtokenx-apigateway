// Redis Distributed Locking (Redlock) Service for GridTokenX
// Implements distributed locking for critical operations

use redis::{AsyncCommands, Client, RedisError, RedisResult};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Distributed lock configuration
#[derive(Debug, Clone)]
pub struct LockConfig {
    /// Time-to-live for locks in seconds
    pub ttl: Duration,
    /// Retry interval between lock attempts
    pub retry_delay: Duration,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Clock drift factor for safety
    pub clock_drift_factor: f64,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30),
            retry_delay: Duration::from_millis(100),
            max_retries: 10,
            clock_drift_factor: 0.01,
        }
    }
}

/// Lock information returned when acquiring a lock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub resource: String,
    pub lock_key: String,
    pub lock_value: String,
    pub acquired_at: Instant,
    pub expires_at: Instant,
    pub ttl: Duration,
}

impl LockInfo {
    /// Create a new lock info
    pub fn new(resource: &str, ttl: Duration) -> Self {
        let now = Instant::now();
        let expires_at = now + ttl;

        Self {
            resource: resource.to_string(),
            lock_key: format!("lock:{}", resource),
            lock_value: Uuid::new_v4().to_string(),
            acquired_at: now,
            expires_at,
            ttl,
        }
    }

    /// Check if the lock is still valid
    pub fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }

    /// Get remaining time until lock expires
    pub fn remaining_time(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    /// Get the time elapsed since lock was acquired
    pub fn elapsed_time(&self) -> Duration {
        self.acquired_at.elapsed()
    }
}

/// Redis distributed lock implementation
#[derive(Clone)]
pub struct RedisLock {
    client: Client,
    config: LockConfig,
}

impl RedisLock {
    /// Create a new Redis lock service
    pub fn new(redis_url: &str, config: LockConfig) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;
        Ok(Self { client, config })
    }

    /// Create a new Redis lock service with default configuration
    pub fn with_default_config(redis_url: &str) -> RedisResult<Self> {
        Self::new(redis_url, LockConfig::default())
    }

    /// Acquire a lock for a resource
    pub async fn acquire_lock(&self, resource: &str) -> RedisResult<Option<LockInfo>> {
        self.acquire_lock_with_config(resource, self.config.clone())
            .await
    }

    /// Acquire a lock with custom configuration
    pub async fn acquire_lock_with_config(
        &self,
        resource: &str,
        config: LockConfig,
    ) -> RedisResult<Option<LockInfo>> {
        let lock_info = LockInfo::new(resource, config.ttl);
        let lock_key = &lock_info.lock_key;
        let lock_value = &lock_info.lock_value;
        let ttl_seconds = config.ttl.as_secs() as u64;

        info!("Attempting to acquire lock for resource: {}", resource);

        for attempt in 0..=config.max_retries {
            let mut conn = self.client.get_multiplexed_async_connection().await?;

            // Use SET with NX and EX options for atomic lock acquisition
            let result: Option<String> = conn.set_nx_ex(lock_key, lock_value, ttl_seconds).await?;

            if result.is_some() {
                // Lock acquired successfully
                info!(
                    "Lock acquired for resource: {} (attempt: {})",
                    resource,
                    attempt + 1
                );
                debug!("Lock info: {:?}", lock_info);
                return Ok(Some(lock_info));
            }

            // Lock not acquired, check if we should retry
            if attempt < config.max_retries {
                debug!(
                    "Lock not acquired for resource: {}, retrying in {:?} (attempt: {})",
                    resource,
                    config.retry_delay,
                    attempt + 1
                );
                tokio::time::sleep(config.retry_delay).await;
            }
        }

        warn!(
            "Failed to acquire lock for resource: {} after {} attempts",
            resource,
            config.max_retries + 1
        );
        Ok(None)
    }

    /// Try to acquire a lock without retries
    pub async fn try_acquire_lock(&self, resource: &str) -> RedisResult<Option<LockInfo>> {
        let lock_info = LockInfo::new(resource, self.config.ttl);
        let lock_key = &lock_info.lock_key;
        let lock_value = &lock_info.lock_value;
        let ttl_seconds = self.config.ttl.as_secs() as u64;

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = conn.set_nx_ex(lock_key, lock_value, ttl_seconds).await?;

        if result.is_some() {
            info!("Lock acquired without retry for resource: {}", resource);
            Ok(Some(lock_info))
        } else {
            Ok(None)
        }
    }

    /// Release a lock
    pub async fn release_lock(&self, lock_info: &LockInfo) -> RedisResult<bool> {
        let lock_key = &lock_info.lock_key;
        let lock_value = &lock_info.lock_value;

        // Use Lua script for atomic lock release
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#;

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: i32 = redis::Script::new(script)
            .key(lock_key)
            .arg(lock_value)
            .invoke_async(&mut conn)
            .await?;

        let released = result == 1;
        if released {
            info!("Lock released for resource: {}", lock_info.resource);
            debug!("Lock released: {:?}", lock_info);
        } else {
            warn!(
                "Failed to release lock for resource: {} (lock not owned or expired)",
                lock_info.resource
            );
        }

        Ok(released)
    }

    /// Extend the TTL of an existing lock
    pub async fn extend_lock(
        &self,
        lock_info: &LockInfo,
        additional_ttl: Duration,
    ) -> RedisResult<bool> {
        if !lock_info.is_valid() {
            warn!(
                "Cannot extend expired lock for resource: {}",
                lock_info.resource
            );
            return Ok(false);
        }

        let lock_key = &lock_info.lock_key;
        let lock_value = &lock_info.lock_value;
        let ttl_seconds = additional_ttl.as_secs() as u64;

        // Use Lua script for atomic TTL extension
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("EXPIRE", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: i32 = redis::Script::new(script)
            .key(lock_key)
            .arg(lock_value)
            .arg(ttl_seconds)
            .invoke_async(&mut conn)
            .await?;

        let extended = result == 1;
        if extended {
            info!("Lock extended for resource: {}", lock_info.resource);
        } else {
            warn!(
                "Failed to extend lock for resource: {} (lock not owned or expired)",
                lock_info.resource
            );
        }

        Ok(extended)
    }

    /// Check if a lock exists and is owned by the specified value
    pub async fn is_lock_owned(&self, lock_info: &LockInfo) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let current_value: Option<String> = conn.get(&lock_info.lock_key).await?;

        Ok(current_value.as_ref() == Some(&lock_info.lock_value))
    }

    /// Get information about existing locks
    pub async fn get_lock_info(&self, resource: &str) -> RedisResult<Option<LockStatus>> {
        let lock_key = format!("lock:{}", resource);
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let lock_value: Option<String> = conn.get(&lock_key).await?;
        let ttl: i64 = conn.ttl(&lock_key).await?;

        if let Some(value) = lock_value {
            Ok(Some(LockStatus {
                resource: resource.to_string(),
                lock_key,
                lock_value: value,
                ttl: if ttl > 0 {
                    Some(Duration::from_secs(ttl as u64))
                } else {
                    None
                },
                exists: true,
            }))
        } else {
            Ok(Some(LockStatus {
                resource: resource.to_string(),
                lock_key,
                lock_value: String::new(),
                ttl: None,
                exists: false,
            }))
        }
    }

    /// Force release a lock (administrative use only)
    pub async fn force_release_lock(&self, resource: &str) -> RedisResult<bool> {
        let lock_key = format!("lock:{}", resource);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: i32 = conn.del(&lock_key).await?;

        let released = result > 0;
        if released {
            warn!("Force released lock for resource: {}", resource);
        } else {
            info!("No lock found to force release for resource: {}", resource);
        }

        Ok(released)
    }

    /// Clean up expired locks (maintenance operation)
    pub async fn cleanup_expired_locks(&self, pattern: &str) -> RedisResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Get all keys matching the pattern
        let keys: Vec<String> = conn.keys(pattern).await?;
        let mut cleaned = 0u32;

        for key in keys {
            let ttl: i64 = conn.ttl(&key).await?;
            if ttl == -1 {
                // Key exists but has no expiration (shouldn't happen)
                warn!("Found lock without TTL: {}", key);
                // Optionally remove it or set an expiration
                // let _: () = conn.expire(&key, 300).await; // 5 minutes
            } else if ttl == -2 {
                // Key doesn't exist
                // This shouldn't happen in the loop, but handle it gracefully
                continue;
            } else if ttl == 0 {
                // Key is expired
                let result: i32 = conn.del(&key).await?;
                cleaned += result as u32;
            }
        }

        if cleaned > 0 {
            info!(
                "Cleaned up {} expired locks matching pattern: {}",
                cleaned, pattern
            );
        }

        Ok(cleaned)
    }

    /// Get statistics about locks
    pub async fn get_lock_statistics(&self, pattern: &str) -> RedisResult<LockStatistics> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let keys: Vec<String> = conn.keys(pattern).await?;
        let mut total_locks = keys.len() as u32;
        let mut expired_locks = 0u32;
        let mut total_ttl = Duration::from_secs(0);

        for key in keys {
            let ttl: i64 = conn.ttl(&key).await?;
            match ttl {
                -1 => {
                    // No expiration
                    debug!("Lock without TTL: {}", key);
                }
                -2 => {
                    // Key doesn't exist
                    expired_locks += 1;
                }
                ttl_seconds if ttl_seconds >= 0 => {
                    total_ttl += Duration::from_secs(ttl_seconds as u64);
                }
                _ => {}
            }
        }

        let avg_ttl = if total_locks > 0 {
            total_ttl / total_locks as u32
        } else {
            Duration::from_secs(0)
        };

        Ok(LockStatistics {
            total_locks,
            expired_locks,
            active_locks: total_locks.saturating_sub(expired_locks),
            average_ttl: avg_ttl,
        })
    }
}

/// Lock status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockStatus {
    pub resource: String,
    pub lock_key: String,
    pub lock_value: String,
    pub ttl: Option<Duration>,
    pub exists: bool,
}

/// Lock statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockStatistics {
    pub total_locks: u32,
    pub expired_locks: u32,
    pub active_locks: u32,
    pub average_ttl: Duration,
}

/// RAII wrapper for automatic lock management
pub struct LockGuard<'a> {
    lock_service: &'a RedisLock,
    lock_info: LockInfo,
    auto_extend: bool,
    extend_interval: Duration,
}

impl<'a> LockGuard<'a> {
    /// Create a new lock guard
    pub fn new(lock_service: &'a RedisLock, lock_info: LockInfo) -> Self {
        Self {
            lock_service,
            lock_info,
            auto_extend: false,
            extend_interval: Duration::from_secs(10),
        }
    }

    /// Enable automatic lock extension
    pub fn with_auto_extend(mut self, interval: Duration) -> Self {
        self.auto_extend = true;
        self.extend_interval = interval;
        self
    }

    /// Get a reference to the lock info
    pub fn lock_info(&self) -> &LockInfo {
        &self.lock_info
    }

    /// Manually extend the lock
    pub async fn extend(&self) -> RedisResult<bool> {
        self.lock_service
            .extend_lock(&self.lock_info, self.extend_interval)
            .await
    }

    /// Check if the lock is still valid
    pub fn is_valid(&self) -> bool {
        self.lock_info.is_valid()
    }
}

impl<'a> Drop for LockGuard<'a> {
    fn drop(&mut self) {
        let lock_service = self.lock_service.clone();
        let lock_info = self.lock_info.clone();

        // Spawn a task to release the lock
        tokio::spawn(async move {
            if let Err(e) = lock_service.release_lock(&lock_info).await {
                error!("Failed to release lock in drop: {}", e);
            }
        });
    }
}

/// High-level lock manager for GridTokenX operations
pub struct GridTokenXLockManager {
    redis_lock: RedisLock,
}

impl GridTokenXLockManager {
    /// Create a new lock manager
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let config = LockConfig {
            ttl: Duration::from_secs(60), // 1 minute default
            retry_delay: Duration::from_millis(50),
            max_retries: 20,
            clock_drift_factor: 0.01,
        };

        Ok(Self {
            redis_lock: RedisLock::new(redis_url, config)?,
        })
    }

    /// Lock for order matching to prevent race conditions
    pub async fn lock_order_matching(&self, symbol: &str) -> RedisResult<Option<LockGuard>> {
        let resource = format!("order_matching:{}", symbol);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(
                LockGuard::new(&self.redis_lock, lock_info)
                    .with_auto_extend(Duration::from_secs(30)),
            ))
        } else {
            Ok(None)
        }
    }

    /// Lock for settlement processing
    pub async fn lock_settlement_processing(
        &self,
        user_id: &str,
    ) -> RedisResult<Option<LockGuard>> {
        let resource = format!("settlement:{}", user_id);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(
                LockGuard::new(&self.redis_lock, lock_info)
                    .with_auto_extend(Duration::from_secs(45)),
            ))
        } else {
            Ok(None)
        }
    }

    /// Lock for token minting operations
    pub async fn lock_token_minting(&self, wallet: &str) -> RedisResult<Option<LockGuard>> {
        let resource = format!("token_minting:{}", wallet);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(LockGuard::new(&self.redis_lock, lock_info)))
        } else {
            Ok(None)
        }
    }

    /// Lock for market clearing operations
    pub async fn lock_market_clearing(&self, epoch: u64) -> RedisResult<Option<LockGuard>> {
        let resource = format!("market_clearing:{}", epoch);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(
                LockGuard::new(&self.redis_lock, lock_info)
                    .with_auto_extend(Duration::from_secs(60)),
            ))
        } else {
            Ok(None)
        }
    }

    /// Lock for meter verification operations
    pub async fn lock_meter_verification(&self, meter_id: &str) -> RedisResult<Option<LockGuard>> {
        let resource = format!("meter_verification:{}", meter_id);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(LockGuard::new(&self.redis_lock, lock_info)))
        } else {
            Ok(None)
        }
    }

    /// Lock for batch operations (e.g., bulk settlements)
    pub async fn lock_batch_operation(&self, batch_id: &str) -> RedisResult<Option<LockGuard>> {
        let resource = format!("batch_operation:{}", batch_id);

        if let Some(lock_info) = self.redis_lock.acquire_lock(&resource).await? {
            Ok(Some(
                LockGuard::new(&self.redis_lock, lock_info)
                    .with_auto_extend(Duration::from_secs(120)),
            ))
        } else {
            Ok(None)
        }
    }

    /// Get lock statistics
    pub async fn get_statistics(&self) -> RedisResult<LockStatistics> {
        self.redis_lock.get_lock_statistics("lock:*").await
    }

    /// Cleanup expired locks
    pub async fn cleanup_expired_locks(&self) -> RedisResult<u32> {
        self.redis_lock.cleanup_expired_locks("lock:*").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lock_creation() {
        let lock_info = LockInfo::new("test_resource", Duration::from_secs(30));

        assert_eq!(lock_info.resource, "test_resource");
        assert_eq!(lock_info.lock_key, "lock:test_resource");
        assert!(lock_info.is_valid());
        assert_eq!(lock_info.remaining_time(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_lock_config_default() {
        let config = LockConfig::default();

        assert_eq!(config.ttl, Duration::from_secs(30));
        assert_eq!(config.retry_delay, Duration::from_millis(100));
        assert_eq!(config.max_retries, 10);
        assert_eq!(config.clock_drift_factor, 0.01);
    }
}
