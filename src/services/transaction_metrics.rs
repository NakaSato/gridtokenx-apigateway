// Metrics for transaction tracking
// Provides metrics collection for transaction monitoring and performance analysis

use crate::models::transaction::TransactionType;
use std::collections::HashMap;
use std::sync::RwLock;
use tokio::sync::OnceCell;

// In-memory counters for pending transactions by type
use std::sync::LazyLock;
static PENDING_TX_COUNTS: LazyLock<RwLock<HashMap<String, i64>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// Metrics initialization
static INIT: LazyLock<OnceCell<()>> = LazyLock::new(OnceCell::new);

/// Initialize all metrics
pub fn init_metrics() {
    if INIT.set(()).is_ok() {
        tracing::info!("Transaction metrics initialized");
    }
}

/// Transaction Metrics Helper
pub struct TransactionMetrics;

impl TransactionMetrics {
    /// Record a new transaction submission
    pub fn record_submission(tx_type: &TransactionType) {
        let tx_type_str = tx_type.to_string();

        // Increment pending counter
        let mut pending_counts = match PENDING_TX_COUNTS.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("PENDING_TX_COUNTS RwLock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        let count = pending_counts.entry(tx_type_str.clone()).or_insert(0);
        *count += 1;

        tracing::debug!("Recorded transaction submission for type: {}", tx_type_str);
    }

    /// Record a transaction confirmation with duration
    pub fn record_confirmation(tx_type: &TransactionType, duration_seconds: f64) {
        let tx_type_str = tx_type.to_string();

        // Decrement pending counter
        let mut pending_counts = match PENDING_TX_COUNTS.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("PENDING_TX_COUNTS RwLock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        if let Some(count) = pending_counts.get_mut(&tx_type_str) {
            if *count > 0 {
                *count -= 1;
            }
        }

        tracing::debug!(
            "Recorded transaction confirmation for type: {}, duration: {}s",
            tx_type_str,
            duration_seconds
        );
    }

    /// Record a transaction failure
    pub fn record_failure(tx_type: &TransactionType, error_type: &str) {
        let tx_type_str = tx_type.to_string();

        // Decrement pending counter if it was pending
        let mut pending_counts = match PENDING_TX_COUNTS.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("PENDING_TX_COUNTS RwLock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        if let Some(count) = pending_counts.get_mut(&tx_type_str) {
            if *count > 0 {
                *count -= 1;
            }
        }

        tracing::debug!(
            "Recorded transaction failure for type: {}, error: {}",
            tx_type_str,
            error_type
        );
    }

    /// Record a retry attempt
    pub fn record_retry(tx_type: &TransactionType) {
        let tx_type_str = tx_type.to_string();
        tracing::debug!("Recorded transaction retry for type: {}", tx_type_str);
    }
}

/// API Metrics Helper
pub struct ApiMetrics;

impl ApiMetrics {
    /// Record an API request
    pub fn record_request(endpoint: &str, method: &str, status_code: u16, duration_seconds: f64) {
        tracing::debug!(
            "API request: {} {} - {} ({}ms)",
            method,
            endpoint,
            status_code,
            duration_seconds * 1000.0
        );
    }
}

/// Database Metrics Helper
pub struct DatabaseMetrics;

impl DatabaseMetrics {
    /// Record a database query
    pub fn record_query(query_type: &str, table: &str, duration_seconds: f64) {
        tracing::debug!(
            "DB query: {} on {} ({}ms)",
            query_type,
            table,
            duration_seconds * 1000.0
        );
    }

    /// Update active connections count
    pub fn set_active_connections(count: i64) {
        tracing::debug!("DB active connections: {}", count);
    }
}

/// Blockchain Metrics Helper
pub struct BlockchainMetrics;

impl BlockchainMetrics {
    /// Record a blockchain RPC call
    pub fn record_rpc_call(method: &str, duration_seconds: f64) {
        tracing::debug!(
            "Blockchain RPC: {} ({}ms)",
            method,
            duration_seconds * 1000.0
        );
    }

    /// Record a blockchain RPC error
    pub fn record_rpc_error(method: &str, error_type: &str) {
        tracing::debug!("Blockchain RPC error: {} - {}", method, error_type);
    }
}

/// Cache Metrics Helper
pub struct CacheMetrics;

impl CacheMetrics {
    /// Record a cache hit
    pub fn record_hit(cache_type: &str) {
        tracing::debug!("Cache hit: {}", cache_type);
    }

    /// Record a cache miss
    pub fn record_miss(cache_type: &str) {
        tracing::debug!("Cache miss: {}", cache_type);
    }
}

/// Metrics Exporter
pub struct MetricsExporter;

impl MetricsExporter {
    /// Get metrics in Prometheus format
    pub fn get_metrics() -> String {
        let pending_counts = match PENDING_TX_COUNTS.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("PENDING_TX_COUNTS RwLock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };

        let mut output = String::new();
        output.push_str(
            "# HELP gridtokenx_transaction_pending_count Current number of pending transactions\n",
        );
        output.push_str("# TYPE gridtokenx_transaction_pending_count gauge\n");

        for (tx_type, count) in pending_counts.iter() {
            output.push_str(&format!(
                "gridtokenx_transaction_pending_count{{tx_type=\"{}\"}} {}\n",
                tx_type, count
            ));
        }

        output
    }

    /// Get structured transaction statistics
    pub fn get_transaction_stats() -> HashMap<String, i64> {
        let pending_counts = match PENDING_TX_COUNTS.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("PENDING_TX_COUNTS RwLock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        pending_counts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transaction::TransactionType;

    #[test]
    fn test_metrics_recording() {
        init_metrics();

        // Test submission
        TransactionMetrics::record_submission(&TransactionType::EnergyTrade);

        // Test confirmation
        TransactionMetrics::record_confirmation(&TransactionType::EnergyTrade, 15.5);

        // Test failure
        TransactionMetrics::record_failure(&TransactionType::EnergyTrade, "network_error");

        // Test retry
        TransactionMetrics::record_retry(&TransactionType::EnergyTrade);

        // Check metrics collection
        let metrics = MetricsExporter::get_metrics();
        assert!(metrics.contains("gridtokenx_transaction_pending_count"));
    }

    #[test]
    fn test_api_metrics_recording() {
        init_metrics();

        // Test API request recording
        ApiMetrics::record_request("/api/v1/transactions/history", "GET", 200, 0.125);

        // Check metrics collection
        let metrics = MetricsExporter::get_metrics();
        assert!(metrics.contains("gridtokenx_transaction_pending_count"));
    }

    #[test]
    fn test_database_metrics_recording() {
        init_metrics();

        // Test DB query recording
        DatabaseMetrics::record_query("SELECT", "blockchain_operations", 0.025);
        DatabaseMetrics::set_active_connections(5);

        // Check metrics collection
        let metrics = MetricsExporter::get_metrics();
        assert!(metrics.contains("gridtokenx_transaction_pending_count"));
    }

    #[test]
    fn test_blockchain_metrics_recording() {
        init_metrics();

        // Test blockchain RPC recording
        BlockchainMetrics::record_rpc_call("getSignatureStatuses", 1.2);
        BlockchainMetrics::record_rpc_error("sendTransaction", "timeout");

        // Check metrics collection
        let metrics = MetricsExporter::get_metrics();
        assert!(metrics.contains("gridtokenx_transaction_pending_count"));
    }

    #[test]
    fn test_cache_metrics_recording() {
        init_metrics();

        // Test cache recording
        CacheMetrics::record_hit("transaction_status");
        CacheMetrics::record_miss("transaction_status");

        // Check metrics collection
        let metrics = MetricsExporter::get_metrics();
        assert!(metrics.contains("gridtokenx_transaction_pending_count"));
    }
}
