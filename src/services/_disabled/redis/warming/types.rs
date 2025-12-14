use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Cache warming strategy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarmingStrategy {
    /// Warm up on application startup
    OnStartup {
        priority: WarmingPriority,
        batch_size: usize,
    },
    /// Warm up on schedule
    Scheduled {
        interval: Duration,
        priority: WarmingPriority,
    },
    /// Warm up based on access patterns
    AccessPattern {
        threshold_accesses: u32,
        time_window: Duration,
    },
    /// Warm up based on demand
    OnDemand {
        pre_fetch_factor: f64,
        trigger_threshold: u32,
    },
}

/// Warming priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarmingPriority {
    Critical = 1,
    High = 2,
    Medium = 3,
    Low = 4,
}

/// Data source types for cache warming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    /// Database query
    Database {
        query: String,
        parameters: HashMap<String, String>,
    },
    /// External API call
    ExternalAPI {
        url: String,
        method: String,
        headers: HashMap<String, String>,
    },
    /// Combined source
    Combined { sources: Vec<String> },
    /// Static data
    Static { data: serde_json::Value },
    /// Computed data
    Computed {
        function: String,
        parameters: HashMap<String, String>,
    },
}

/// Retry configuration for warming tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_factor: 2.0,
        }
    }
}

/// Cache warming task definition
#[derive(Debug, Clone)]
pub struct WarmingTask {
    pub id: String,
    pub key_pattern: String,
    pub strategy: WarmingStrategy,
    pub data_source: DataSource,
    pub ttl: Option<Duration>,
    pub dependencies: Vec<String>,
    pub retry_config: RetryConfig,
}

impl WarmingTask {
    /// Create a new warming task
    pub fn new(
        id: &str,
        key_pattern: &str,
        strategy: WarmingStrategy,
        data_source: DataSource,
    ) -> Self {
        Self {
            id: id.to_string(),
            key_pattern: key_pattern.to_string(),
            strategy,
            data_source,
            ttl: None,
            dependencies: Vec::new(),
            retry_config: RetryConfig::default(),
        }
    }

    /// Set TTL for warmed cache entries
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Add dependencies
    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    /// Set retry configuration
    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }
}

/// Warming task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingResult {
    pub task_id: String,
    pub success: bool,
    pub warmed_keys: u32,
    pub duration: Duration,
    pub error_message: Option<String>,
    pub timestamp: u64,
}

impl WarmingResult {
    /// Create successful result
    pub fn success(task_id: &str, warmed_keys: u32, duration: Duration) -> Self {
        Self {
            task_id: task_id.to_string(),
            success: true,
            warmed_keys,
            duration,
            error_message: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create failed result
    pub fn failure(task_id: &str, error_message: &str, duration: Duration) -> Self {
        Self {
            task_id: task_id.to_string(),
            success: false,
            warmed_keys: 0,
            duration,
            error_message: Some(error_message.to_string()),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Warming statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingStatistics {
    pub total_tasks: usize,
    pub recent_executions: usize,
    pub success_rate: f64,
    pub total_warmed_keys: u32,
    pub average_execution_time: Duration,
}
