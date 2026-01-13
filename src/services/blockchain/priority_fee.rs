//! Priority Fee Service
//! 
//! Dynamically calculates priority fees based on recent network activity.

use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Transaction types for fee calculation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionType {
    TokenTransfer,
    Minting,
    Trading,
    Settlement,
    Other,
}

impl TransactionType {
    /// Get the priority fee multiplier for this transaction type
    pub fn multiplier(&self) -> f64 {
        match self {
            TransactionType::TokenTransfer => 1.0,
            TransactionType::Minting => 1.5,
            TransactionType::Trading => 2.0,
            TransactionType::Settlement => 2.5,
            TransactionType::Other => 1.0,
        }
    }
}

/// Cached priority fee data
#[derive(Debug, Clone)]
struct CachedFee {
    base_fee: u64,
    timestamp: Instant,
}

/// Service for calculating dynamic priority fees
#[derive(Clone)]
pub struct PriorityFeeService {
    rpc_client: Arc<RpcClient>,
    cache: Arc<RwLock<Option<CachedFee>>>,
    cache_ttl: Duration,
    default_fee: u64,
    min_fee: u64,
    max_fee: u64,
}

impl std::fmt::Debug for PriorityFeeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriorityFeeService")
            .field("default_fee", &self.default_fee)
            .field("cache_ttl", &self.cache_ttl)
            .finish()
    }
}

impl PriorityFeeService {
    /// Create a new priority fee service
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            cache: Arc::new(RwLock::new(None)),
            cache_ttl: Duration::from_secs(10),
            default_fee: 10_000,       // 0.00001 SOL = 10,000 micro-lamports
            min_fee: 1_000,            // Minimum fee
            max_fee: 1_000_000,        // Maximum fee (1 SOL in micro-lamports)
        }
    }

    /// Get the recommended priority fee for a transaction type
    pub async fn get_priority_fee(&self, tx_type: TransactionType) -> Result<u64> {
        let base_fee = self.get_base_priority_fee().await?;
        let multiplier = tx_type.multiplier();
        let adjusted_fee = (base_fee as f64 * multiplier) as u64;
        
        // Clamp to min/max bounds
        let final_fee = adjusted_fee.clamp(self.min_fee, self.max_fee);
        
        debug!(
            "Priority fee for {:?}: base={}, multiplier={:.1}x, adjusted={}, final={}",
            tx_type, base_fee, multiplier, adjusted_fee, final_fee
        );
        
        Ok(final_fee)
    }

    /// Get base priority fee from network or cache
    async fn get_base_priority_fee(&self) -> Result<u64> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.timestamp.elapsed() < self.cache_ttl {
                    debug!("Using cached priority fee: {}", cached.base_fee);
                    return Ok(cached.base_fee);
                }
            }
        }

        // Fetch from network
        let fee = self.fetch_priority_fee_from_network().await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedFee {
                base_fee: fee,
                timestamp: Instant::now(),
            });
        }

        Ok(fee)
    }

    /// Fetch priority fee from the network using recent prioritization fees
    async fn fetch_priority_fee_from_network(&self) -> Result<u64> {
        match self.rpc_client.get_recent_prioritization_fees(&[]) {
            Ok(fees) => {
                if fees.is_empty() {
                    info!("No recent priority fees found, using default: {}", self.default_fee);
                    return Ok(self.default_fee);
                }

                // Collect non-zero fees
                let mut fee_values: Vec<u64> = fees
                    .iter()
                    .map(|f| f.prioritization_fee)
                    .filter(|&f| f > 0)
                    .collect();

                if fee_values.is_empty() {
                    return Ok(self.default_fee);
                }

                fee_values.sort();

                // Calculate P75 (75th percentile) for reliability
                let p75_index = (fee_values.len() * 75) / 100;
                let p75_fee = fee_values.get(p75_index).copied().unwrap_or(self.default_fee);

                // Add 20% buffer for reliability
                let buffered_fee = p75_fee.saturating_mul(120) / 100;

                info!(
                    "Fetched priority fees: count={}, median={}, p75={}, buffered={}",
                    fee_values.len(),
                    fee_values[fee_values.len() / 2],
                    p75_fee,
                    buffered_fee
                );

                Ok(buffered_fee)
            }
            Err(e) => {
                warn!("Failed to fetch priority fees: {}, using default", e);
                Ok(self.default_fee)
            }
        }
    }

    /// Clear the fee cache (useful after network conditions change)
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
        debug!("Priority fee cache cleared");
    }
}
