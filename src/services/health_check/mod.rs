use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub mod types;
pub use types::{DependencyHealth, DetailedHealthStatus, HealthCheckStatus, SystemMetrics};

/// Health checker service
#[derive(Clone)]
pub struct HealthChecker {
    start_time: Arc<Instant>,
    db_pool: sqlx::PgPool,
    redis_client: redis::Client,
    blockchain_url: String,
    last_check: Arc<RwLock<Option<DetailedHealthStatus>>>,
    email_service_enabled: bool,
}

impl HealthChecker {
    pub fn new(
        db_pool: sqlx::PgPool,
        redis_client: redis::Client,
        blockchain_url: String,
        email_service_enabled: bool,
    ) -> Self {
        Self {
            start_time: Arc::new(Instant::now()),
            db_pool,
            redis_client,
            blockchain_url,
            last_check: Arc::new(RwLock::new(None)),
            email_service_enabled,
        }
    }

    /// Get uptime in seconds
    pub fn get_uptime(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Check database health
    async fn check_database(&self) -> DependencyHealth {
        let start = Instant::now();

        match sqlx::query("SELECT 1").fetch_one(&self.db_pool).await {
            Ok(_) => DependencyHealth {
                name: "PostgreSQL".to_string(),
                status: HealthCheckStatus::Healthy,
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                last_check: Utc::now(),
                error_message: None,
                details: Some("Database connection successful".to_string()),
            },
            Err(e) => DependencyHealth {
                name: "PostgreSQL".to_string(),
                status: HealthCheckStatus::Unhealthy,
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                last_check: Utc::now(),
                error_message: Some(e.to_string()),
                details: None,
            },
        }
    }

    /// Check Redis health
    async fn check_redis(&self) -> DependencyHealth {
        let start = Instant::now();

        match self.redis_client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                use redis::AsyncCommands;
                match conn.get::<&str, Option<String>>("health_check").await {
                    Ok(_) => DependencyHealth {
                        name: "Redis".to_string(),
                        status: HealthCheckStatus::Healthy,
                        response_time_ms: Some(start.elapsed().as_millis() as u64),
                        last_check: Utc::now(),
                        error_message: None,
                        details: Some("Redis connection successful".to_string()),
                    },
                    Err(e) => DependencyHealth {
                        name: "Redis".to_string(),
                        status: HealthCheckStatus::Unhealthy,
                        response_time_ms: Some(start.elapsed().as_millis() as u64),
                        last_check: Utc::now(),
                        error_message: Some(e.to_string()),
                        details: None,
                    },
                }
            }
            Err(e) => DependencyHealth {
                name: "Redis".to_string(),
                status: HealthCheckStatus::Unhealthy,
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                last_check: Utc::now(),
                error_message: Some(e.to_string()),
                details: None,
            },
        }
    }

    /// Check blockchain RPC health
    async fn check_blockchain(&self) -> DependencyHealth {
        let start = Instant::now();

        match reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            Ok(client) => {
                match client
                    .post(&self.blockchain_url)
                    .json(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "getHealth"
                    }))
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            DependencyHealth {
                                name: "Solana RPC".to_string(),
                                status: HealthCheckStatus::Healthy,
                                response_time_ms: Some(start.elapsed().as_millis() as u64),
                                last_check: Utc::now(),
                                error_message: None,
                                details: Some("RPC endpoint responding".to_string()),
                            }
                        } else {
                            DependencyHealth {
                                name: "Solana RPC".to_string(),
                                status: HealthCheckStatus::Degraded,
                                response_time_ms: Some(start.elapsed().as_millis() as u64),
                                last_check: Utc::now(),
                                error_message: Some(format!("HTTP {}", response.status())),
                                details: None,
                            }
                        }
                    }
                    Err(e) => DependencyHealth {
                        name: "Solana RPC".to_string(),
                        status: HealthCheckStatus::Unhealthy,
                        response_time_ms: Some(start.elapsed().as_millis() as u64),
                        last_check: Utc::now(),
                        error_message: Some(e.to_string()),
                        details: None,
                    },
                }
            }
            Err(e) => DependencyHealth {
                name: "Solana RPC".to_string(),
                status: HealthCheckStatus::Unhealthy,
                response_time_ms: None,
                last_check: Utc::now(),
                error_message: Some(e.to_string()),
                details: None,
            },
        }
    }

    /// Check email service health
    fn check_email(&self) -> DependencyHealth {
        if self.email_service_enabled {
            DependencyHealth {
                name: "Email Service".to_string(),
                status: HealthCheckStatus::Healthy,
                response_time_ms: None,
                last_check: Utc::now(),
                error_message: None,
                details: Some("Email service is configured and enabled".to_string()),
            }
        } else {
            DependencyHealth {
                name: "Email Service".to_string(),
                status: HealthCheckStatus::Degraded, // Or another status if totally disabled is "normal"
                response_time_ms: None,
                last_check: Utc::now(),
                error_message: Some("Email service is NOT configured".to_string()),
                details: None,
            }
        }
    }

    /// Get system metrics
    fn get_system_metrics(&self) -> SystemMetrics {
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_cpu_all();
        sys.refresh_memory();

        SystemMetrics {
            cpu_usage: Some(sys.global_cpu_usage() as f64),
            memory_used_mb: Some(sys.used_memory() / 1024 / 1024),
            memory_total_mb: Some(sys.total_memory() / 1024 / 1024),
            disk_used_gb: None,
            disk_total_gb: None,
            active_connections: 0, // Would track in middleware
        }
    }

    /// Perform full health check
    pub async fn perform_health_check(&self) -> DetailedHealthStatus {
        // Check all dependencies in parallel
        let (db_health, redis_health, blockchain_health) = tokio::join!(
            self.check_database(),
            self.check_redis(),
            self.check_blockchain()
        );

        let email_health = self.check_email();
        let dependencies = vec![db_health, redis_health, blockchain_health, email_health];

        // Determine overall status
        let overall_status = if dependencies
            .iter()
            .all(|d| d.status == HealthCheckStatus::Healthy)
        {
            "healthy"
        } else if dependencies
            .iter()
            .any(|d| d.status == HealthCheckStatus::Unhealthy)
        {
            "unhealthy"
        } else {
            "degraded"
        };

        let status = DetailedHealthStatus {
            status: overall_status.to_string(),
            timestamp: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            uptime_seconds: self.get_uptime(),
            dependencies,
            metrics: self.get_system_metrics(),
        };

        // Cache the result
        *self.last_check.write().await = Some(status.clone());

        status
    }

    /// Get cached health check result
    pub async fn get_cached_health(&self) -> Option<DetailedHealthStatus> {
        self.last_check.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_status() {
        assert_eq!(HealthCheckStatus::Healthy, HealthCheckStatus::Healthy);
        assert_ne!(HealthCheckStatus::Healthy, HealthCheckStatus::Unhealthy);
    }

    #[test]
    fn test_system_metrics_serialization() {
        let metrics = SystemMetrics {
            cpu_usage: Some(45.5),
            memory_used_mb: Some(1024),
            memory_total_mb: Some(8192),
            disk_used_gb: Some(100),
            disk_total_gb: Some(500),
            active_connections: 42,
        };

        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("cpu_usage"));
    }
}
