use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn, info, error};

/// Zone rate configuration from database
#[derive(Clone, Debug)]
pub struct ZoneRate {
    pub from_zone_id: i32,
    pub to_zone_id: i32,
    pub wheeling_charge: Decimal,
    pub loss_factor: Decimal,
}

/// Service to manage grid topology and calculate transmission costs
#[derive(Clone)]
pub struct GridTopologyService {
    /// Cached zone rates: (from_zone, to_zone) -> ZoneRate
    rates_cache: Arc<RwLock<HashMap<(i32, i32), ZoneRate>>>,
    /// Database pool for loading rates
    pool: Option<PgPool>,
    /// Last cache refresh timestamp
    last_refresh: Arc<RwLock<Option<std::time::Instant>>>,
}

impl GridTopologyService {
    pub fn new() -> Self {
        Self {
            rates_cache: Arc::new(RwLock::new(HashMap::new())),
            pool: None,
            last_refresh: Arc::new(RwLock::new(None)),
        }
    }

    /// Create service with database connection for dynamic rates
    pub fn with_pool(pool: PgPool) -> Self {
        Self {
            rates_cache: Arc::new(RwLock::new(HashMap::new())),
            pool: Some(pool),
            last_refresh: Arc::new(RwLock::new(None)),
        }
    }

    /// Spawn a background task to refresh cache periodically
    /// Returns the JoinHandle for the spawned task
    pub fn spawn_refresh_task(self: Arc<Self>, refresh_interval_secs: u64) -> tokio::task::JoinHandle<()> {
        let interval = Duration::from_secs(refresh_interval_secs);
        info!("🔄 Starting zone rates cache refresh task (interval: {}s)", refresh_interval_secs);
        
        tokio::spawn(async move {
            // Initial load
            if let Err(e) = self.load_rates().await {
                error!("Failed initial zone rates load: {}", e);
            }

            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await; // Skip first immediate tick

            loop {
                interval_timer.tick().await;
                match self.load_rates().await {
                    Ok(count) => {
                        info!("🔄 Refreshed zone rates cache: {} rates loaded", count);
                    }
                    Err(e) => {
                        error!("Failed to refresh zone rates cache: {}", e);
                    }
                }
            }
        })
    }

    /// Load zone rates from database into cache
    pub async fn load_rates(&self) -> Result<usize, sqlx::Error> {
        let Some(pool) = &self.pool else {
            warn!("No database pool configured, using default rates");
            return Ok(0);
        };

        let rows = sqlx::query(
            r#"
            SELECT from_zone_id, to_zone_id, wheeling_charge, loss_factor
            FROM zone_rates
            WHERE is_active = TRUE
              AND (effective_until IS NULL OR effective_until > NOW())
              AND effective_from <= NOW()
            "#
        )
        .fetch_all(pool)
        .await?;

        let count = rows.len();
        let mut cache = self.rates_cache.write().await;
        cache.clear();
        for row in rows {
            use sqlx::Row;
            let rate = ZoneRate {
                from_zone_id: row.get("from_zone_id"),
                to_zone_id: row.get("to_zone_id"),
                wheeling_charge: row.get("wheeling_charge"),
                loss_factor: row.get("loss_factor"),
            };
            cache.insert((rate.from_zone_id, rate.to_zone_id), rate);
        }
        
        // Update last refresh timestamp
        *self.last_refresh.write().await = Some(std::time::Instant::now());
        debug!("Loaded {} zone rates from database", count);
        Ok(count)
    }

    /// Get cache age in seconds (None if never refreshed)
    pub async fn cache_age_secs(&self) -> Option<u64> {
        self.last_refresh.read().await.map(|t| t.elapsed().as_secs())
    }

    /// Get rate from cache or use default
    async fn get_rate(&self, from_zone: i32, to_zone: i32) -> Option<ZoneRate> {
        let cache = self.rates_cache.read().await;
        cache.get(&(from_zone, to_zone)).cloned()
    }

    /// Calculate wheeling charge (transmission fee) in THB per kWh
    /// returns: Fee in THB
    pub fn calculate_wheeling_charge(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        // Try to get from cache synchronously for backward compatibility
        // For async version, use calculate_wheeling_charge_async
        self.calculate_wheeling_charge_default(from_zone, to_zone)
    }

    /// Async version that checks database cache first
    pub async fn calculate_wheeling_charge_async(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(fz), Some(tz)) => {
                if let Some(rate) = self.get_rate(fz, tz).await {
                    return rate.wheeling_charge;
                }
                self.calculate_wheeling_charge_default(from_zone, to_zone)
            }
            _ => self.calculate_wheeling_charge_default(from_zone, to_zone),
        }
    }

    /// Default wheeling charge calculation (fallback)
    fn calculate_wheeling_charge_default(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(mz), Some(bz)) => {
                if mz == bz {
                    // Local distribution fee only
                    Decimal::from_f64(0.50).expect("hardcoded decimal 0.50")
                } else {
                    let distance = (mz - bz).abs();
                    if distance == 1 {
                        // Adjacent zone
                        Decimal::from_f64(1.00).expect("hardcoded decimal 1.00")
                    } else {
                        // Cross-zone transmission
                        Decimal::from_f64(1.50).expect("hardcoded decimal 1.50") + Decimal::from(distance) * Decimal::from_f64(0.1).expect("hardcoded decimal 0.1")
                    }
                }
            }
            _ => {
                // Default high fee if zones unknown
                Decimal::from_f64(2.00).expect("hardcoded decimal 2.00")
            }
        }
    }

    /// Calculate technical loss (%)
    /// returns: Percentage as Decimal (e.g., 0.03 for 3%)
    pub fn calculate_loss_factor(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        self.calculate_loss_factor_default(from_zone, to_zone)
    }

    /// Async version that checks database cache first
    pub async fn calculate_loss_factor_async(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(fz), Some(tz)) => {
                if let Some(rate) = self.get_rate(fz, tz).await {
                    return rate.loss_factor;
                }
                self.calculate_loss_factor_default(from_zone, to_zone)
            }
            _ => self.calculate_loss_factor_default(from_zone, to_zone),
        }
    }

    /// Default loss factor calculation (fallback)
    fn calculate_loss_factor_default(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(mz), Some(bz)) => {
                if mz == bz {
                    // Minimal local loss
                    Decimal::from_f64(0.01).expect("hardcoded decimal 0.01")
                } else {
                    let distance = (mz - bz).abs();
                    if distance == 1 {
                        Decimal::from_f64(0.03).expect("hardcoded decimal 0.03")
                    } else {
                        // Max cap at 15%
                        let loss = 0.03 + (distance as f64 * 0.01);
                        Decimal::from_f64(loss.min(0.15)).expect("loss calculation overflow")
                    }
                }
            }
            _ => {
                // Conservative default
                Decimal::from_f64(0.05).expect("hardcoded decimal 0.05")
            }
        }
    }

    /// Calculate actual cost of losses for a given energy amount and price
    /// Loss Cost = Energy * LossFactor * EnergyPrice
    pub fn calculate_loss_cost(&self, energy_amount: Decimal, price: Decimal, loss_factor: Decimal) -> Decimal {
        energy_amount * price * loss_factor
    }
}

impl std::fmt::Debug for GridTopologyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridTopologyService")
            .field("has_pool", &self.pool.is_some())
            .finish()
    }
}


