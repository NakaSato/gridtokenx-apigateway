// Cache Warming Strategies for GridTokenX
// Implements proactive cache population and warming patterns

use redis::{AsyncCommands, Client, RedisError, RedisResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Cache warming strategy types
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
    /// Static data
    Static { data: serde_json::Value },
    /// Computed data
    Computed {
        function: String,
        parameters: HashMap<String, String>,
    },
}

/// Retry configuration for warming tasks
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub backoff_factor: f64,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Warming task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingResult {
    pub task_id: String,
    pub success: bool,
    pub warmed_keys: u32,
    pub failed_keys: u32,
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
            failed_keys: 0,
            duration,
            error_message: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Create failed result
    pub fn failure(task_id: &str, error_message: &str, duration: Duration) -> Self {
        Self {
            task_id: task_id.to_string(),
            success: false,
            warmed_keys: 0,
            failed_keys: 0,
            duration,
            error_message: Some(error_message.to_string()),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

/// Cache warming service
pub struct RedisCacheWarmer {
    client: Client,
    tasks: HashMap<String, WarmingTask>,
    execution_history: Vec<WarmingResult>,
    max_history_size: usize,
}

impl RedisCacheWarmer {
    /// Create a new cache warmer
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;
        Ok(Self {
            client,
            tasks: HashMap::new(),
            execution_history: Vec::new(),
            max_history_size: 1000,
        })
    }

    /// Add a warming task
    pub fn add_task(&mut self, task: WarmingTask) {
        let task_id = task.id.clone();
        self.tasks.insert(task_id.clone(), task);
        info!("Added warming task: {}", task_id);
    }

    /// Execute a specific warming task
    pub async fn execute_task(&self, task_id: &str) -> RedisResult<WarmingResult> {
        let task = self.tasks.get(task_id).ok_or_else(|| {
            RedisError::from((
                redis::ErrorKind::TypeError,
                "Task not found",
                task_id.to_string(),
            ))
        })?;

        let start_time = SystemTime::now();
        info!("Executing warming task: {}", task_id);

        // Check dependencies first
        for dependency in &task.dependencies {
            if !self.check_dependency(dependency).await? {
                warn!(
                    "Dependency not satisfied for task {}: {}",
                    task_id, dependency
                );
                return Ok(WarmingResult::failure(
                    task_id,
                    &format!("Dependency not satisfied: {}", dependency),
                    start_time.elapsed().unwrap_or_default(),
                ));
            }
        }

        // Execute based on strategy
        let result = match &task.data_source {
            DataSource::Database { query, parameters } => {
                self.warm_from_database(task, query, parameters).await?
            }
            DataSource::ExternalAPI {
                url,
                method,
                headers,
            } => {
                self.warm_from_external_api(task, url, method, headers)
                    .await?
            }
            DataSource::Static { data } => self.warm_from_static_data(task, data).await?,
            DataSource::Computed {
                function,
                parameters,
            } => {
                self.warm_from_computed_data(task, function, parameters)
                    .await?
            }
        };

        // Record execution
        self.record_execution(result.clone()).await;

        info!(
            "Warming task {} completed: {} keys warmed",
            task_id, result.warmed_keys
        );
        Ok(result)
    }

    /// Warm cache from database
    async fn warm_from_database(
        &self,
        task: &WarmingTask,
        query: &str,
        parameters: &HashMap<String, String>,
    ) -> RedisResult<WarmingResult> {
        let start_time = SystemTime::now();
        let mut warmed_keys = 0u32;
        let mut failed_keys = 0u32;

        // In a real implementation, this would execute the database query
        // For this example, we'll simulate database data
        let simulated_data = self.simulate_database_query(query, parameters).await?;

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        for (key, value) in simulated_data {
            match self
                .set_cache_value(&mut conn, &task.key_pattern, &key, &value, task.ttl)
                .await
            {
                Ok(_) => {
                    warmed_keys += 1;
                    debug!("Warmed cache key: {}", key);
                }
                Err(e) => {
                    failed_keys += 1;
                    error!("Failed to warm cache key {}: {}", key, e);
                }
            }
        }

        Ok(WarmingResult::success(
            &task.id,
            warmed_keys,
            start_time.elapsed().unwrap_or_default(),
        ))
    }

    /// Warm cache from external API
    async fn warm_from_external_api(
        &self,
        task: &WarmingTask,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
    ) -> RedisResult<WarmingResult> {
        let start_time = SystemTime::now();
        let mut warmed_keys = 0u32;
        let mut failed_keys = 0u32;

        // Simulate API call with retry logic
        let api_data = self
            .fetch_with_retry(url, method, headers, &task.retry_config)
            .await?;

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        for (key, value) in api_data {
            match self
                .set_cache_value(&mut conn, &task.key_pattern, &key, &value, task.ttl)
                .await
            {
                Ok(_) => {
                    warmed_keys += 1;
                    debug!("Warmed cache key from API: {}", key);
                }
                Err(e) => {
                    failed_keys += 1;
                    error!("Failed to warm cache key from API {}: {}", key, e);
                }
            }
        }

        Ok(WarmingResult::success(
            &task.id,
            warmed_keys,
            start_time.elapsed().unwrap_or_default(),
        ))
    }

    /// Warm cache from static data
    async fn warm_from_static_data(
        &self,
        task: &WarmingTask,
        data: &serde_json::Value,
    ) -> RedisResult<WarmingResult> {
        let start_time = SystemTime::now();
        let mut warmed_keys = 0u32;
        let mut failed_keys = 0u32;

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Generate keys and values from static data
        if let Some(obj) = data.as_object() {
            for (key, value) in obj {
                let cache_key = self.generate_key_from_pattern(&task.key_pattern, key);
                match self
                    .set_cache_value(&mut conn, &task.key_pattern, &cache_key, value, task.ttl)
                    .await
                {
                    Ok(_) => {
                        warmed_keys += 1;
                        debug!("Warmed static cache key: {}", cache_key);
                    }
                    Err(e) => {
                        failed_keys += 1;
                        error!("Failed to warm static cache key {}: {}", cache_key, e);
                    }
                }
            }
        }

        Ok(WarmingResult::success(
            &task.id,
            warmed_keys,
            start_time.elapsed().unwrap_or_default(),
        ))
    }

    /// Warm cache from computed data
    async fn warm_from_computed_data(
        &self,
        task: &WarmingTask,
        function: &str,
        parameters: &HashMap<String, String>,
    ) -> RedisResult<WarmingResult> {
        let start_time = SystemTime::now();
        let mut warmed_keys = 0u32;
        let mut failed_keys = 0u32;

        // Execute function to generate data
        let computed_data = self.execute_computation(function, parameters).await?;

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        for (key, value) in computed_data {
            match self
                .set_cache_value(&mut conn, &task.key_pattern, &key, &value, task.ttl)
                .await
            {
                Ok(_) => {
                    warmed_keys += 1;
                    debug!("Warmed computed cache key: {}", key);
                }
                Err(e) => {
                    failed_keys += 1;
                    error!("Failed to warm computed cache key {}: {}", key, e);
                }
            }
        }

        Ok(WarmingResult::success(
            &task.id,
            warmed_keys,
            start_time.elapsed().unwrap_or_default(),
        ))
    }

    /// Set cache value with proper key generation
    async fn set_cache_value(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
        pattern: &str,
        key: &str,
        value: &serde_json::Value,
        ttl: Option<Duration>,
    ) -> RedisResult<()> {
        let cache_key = self.generate_key_from_pattern(pattern, key);
        let value_json = serde_json::to_string(value).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Serialization error",
                e.to_string(),
            ))
        })?;

        let _: () = conn.set(&cache_key, &value_json).await?;

        if let Some(ttl) = ttl {
            let _: () = conn.expire(&cache_key, ttl.as_secs() as u64).await?;
        }

        Ok(())
    }

    /// Generate cache key from pattern
    fn generate_key_from_pattern(&self, pattern: &str, key: &str) -> String {
        if pattern.contains("{}") {
            pattern.replace("{}", key)
        } else {
            format!("{}:{}", pattern, key)
        }
    }

    /// Check if dependency is satisfied
    async fn check_dependency(&self, dependency: &str) -> RedisResult<bool> {
        // Check if dependency task was executed successfully
        let key = format!("warming_dependency:{}", dependency);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = conn.get(&key).await?;

        Ok(result.is_some())
    }

    /// Record task execution
    async fn record_execution(&mut self, result: WarmingResult) {
        // Add to history
        self.execution_history.push(result.clone());

        // Trim history if needed
        if self.execution_history.len() > self.max_history_size {
            self.execution_history.remove(0);
        }

        // Record in Redis for persistence
        let key = format!("warming_history:{}", result.task_id);
        let value = serde_json::to_string(&result).unwrap_or_default();

        if let Ok(mut conn) = self.client.get_multiplexed_async_connection().await {
            let _: () = conn.set(&key, &value).await;
            let _: () = conn.expire(&key, 86400).await; // 24 hours
        }

        // Mark dependencies as satisfied
        if result.success {
            let dep_key = format!("warming_dependency:{}", result.task_id);
            if let Ok(mut conn) = self.client.get_multiplexed_async_connection().await {
                let _: () = conn.set(&dep_key, "completed").await;
                let _: () = conn.expire(&dep_key, 3600).await; // 1 hour
            }
        }
    }

    /// Execute all startup tasks
    pub async fn execute_startup_tasks(&self) -> RedisResult<Vec<WarmingResult>> {
        let startup_tasks: Vec<&WarmingTask> = self
            .tasks
            .values()
            .filter(|task| matches!(&task.strategy, WarmingStrategy::OnStartup { .. }))
            .collect();

        // Sort by priority
        let mut sorted_tasks = startup_tasks;
        sorted_tasks.sort_by(|a, b| {
            let priority_a = match &a.strategy {
                WarmingStrategy::OnStartup { priority, .. } => priority,
                _ => &WarmingPriority::Low,
            };
            let priority_b = match &b.strategy {
                WarmingStrategy::OnStartup { priority, .. } => priority,
                _ => &WarmingPriority::Low,
            };
            priority_a.cmp(priority_b)
        });

        let mut results = Vec::new();
        for task in sorted_tasks {
            let result = self.execute_task(&task.id).await?;
            results.push(result);
        }

        info!("Executed {} startup warming tasks", results.len());
        Ok(results)
    }

    /// Get warming statistics
    pub async fn get_warming_statistics(&self) -> RedisResult<WarmingStatistics> {
        let total_tasks = self.tasks.len();
        let recent_executions = self
            .execution_history
            .iter()
            .filter(|r| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .saturating_sub(r.timestamp)
                    < 3600 // Last hour
            })
            .count();

        let success_rate = if !self.execution_history.is_empty() {
            let successful = self.execution_history.iter().filter(|r| r.success).count() as f64;
            successful / self.execution_history.len() as f64 * 100.0
        } else {
            0.0
        };

        let total_warmed_keys: u32 = self.execution_history.iter().map(|r| r.warmed_keys).sum();

        Ok(WarmingStatistics {
            total_tasks,
            recent_executions,
            success_rate,
            total_warmed_keys,
            average_execution_time: self.calculate_average_execution_time(),
        })
    }

    /// Calculate average execution time
    fn calculate_average_execution_time(&self) -> Duration {
        if self.execution_history.is_empty() {
            return Duration::from_secs(0);
        }

        let total_duration: Duration = self.execution_history.iter().map(|r| r.duration).sum();

        total_duration / self.execution_history.len() as u32
    }

    /// Simulate database query (placeholder implementation)
    async fn simulate_database_query(
        &self,
        query: &str,
        parameters: &HashMap<String, String>,
    ) -> RedisResult<HashMap<String, serde_json::Value>> {
        // In a real implementation, this would execute the actual database query
        debug!(
            "Simulating database query: {} with params: {:?}",
            query, parameters
        );

        let mut data = HashMap::new();

        // Generate some sample data based on query type
        if query.contains("user_profile") {
            for i in 1..=10 {
                let key = format!("user_{}", i);
                let value = serde_json::json!({
                    "id": i,
                    "name": format!("User {}", i),
                    "email": format!("user{}@example.com", i),
                    "created_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                });
                data.insert(key, value);
            }
        } else if query.contains("market_data") {
            for i in 1..=5 {
                let key = format!("symbol_{}", i);
                let value = serde_json::json!({
                    "symbol": format!("SYMBOL{}", i),
                    "price": 0.25 + (i as f64 * 0.01),
                    "volume": 1000 * i,
                    "last_updated": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                });
                data.insert(key, value);
            }
        }

        Ok(data)
    }

    /// Fetch data from external API with retry logic
    async fn fetch_with_retry(
        &self,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
        retry_config: &RetryConfig,
    ) -> RedisResult<HashMap<String, serde_json::Value>> {
        debug!("Fetching from API: {} {}", method, url);

        let mut delay = retry_config.base_delay;

        for attempt in 0..=retry_config.max_retries {
            // In a real implementation, this would make actual HTTP requests
            // For this example, we'll simulate API data
            let simulated_data = self.simulate_api_response(url).await;

            if !simulated_data.is_empty() {
                return Ok(simulated_data);
            }

            if attempt < retry_config.max_retries {
                warn!(
                    "API request failed, retrying in {:?} (attempt {})",
                    delay,
                    attempt + 1
                );
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(
                    Duration::from_millis(
                        (delay.as_millis() as f64 * retry_config.backoff_factor) as u64,
                    ),
                    retry_config.max_delay,
                );
            }
        }

        Err(RedisError::from((
            redis::ErrorKind::IoError,
            "API request failed after all retries",
            "API request failed after all retries".to_string(),
        )))
    }

    /// Simulate API response
    async fn simulate_api_response(&self, url: &str) -> HashMap<String, serde_json::Value> {
        let mut data = HashMap::new();

        // Generate sample data based on URL
        if url.contains("market") {
            for i in 1..=10 {
                let key = format!("market_{}", i);
                let value = serde_json::json!({
                    "id": i,
                    "name": format!("Market {}", i),
                    "price": (100 + i) as f64,
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                });
                data.insert(key, value);
            }
        }

        data
    }

    /// Execute computation function
    async fn execute_computation(
        &self,
        function: &str,
        parameters: &HashMap<String, String>,
    ) -> RedisResult<HashMap<String, serde_json::Value>> {
        debug!(
            "Executing computation: {} with params: {:?}",
            function, parameters
        );

        let mut data = HashMap::new();

        // Sample computations based on function name
        match function {
            "calculate_moving_averages" => {
                for i in 1..=5 {
                    let key = format!("ma_{}", i);
                    let value = serde_json::json!({
                        "period": i * 10,
                        "average": 0.25 + (i as f64 * 0.01),
                        "calculated_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                    });
                    data.insert(key, value);
                }
            }
            "precompute_trading_stats" => {
                for i in 1..=3 {
                    let key = format!("stats_{}", i);
                    let value = serde_json::json!({
                        "period": format!("period_{}", i),
                        "total_volume": 10000 * i,
                        "total_trades": 100 * i,
                        "average_price": 0.25,
                        "calculated_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                    });
                    data.insert(key, value);
                }
            }
            _ => {}
        }

        Ok(data)
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

/// Pre-configured warming tasks for GridTokenX
pub struct GridTokenXCacheWarmer {
    warmer: RedisCacheWarmer,
}

impl GridTokenXCacheWarmer {
    /// Create GridTokenX cache warmer with standard tasks
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let mut warmer = RedisCacheWarmer::new(redis_url)?;

        // User profile warming
        warmer.add_task(
            WarmingTask::new(
                "warm_user_profiles",
                "user_profile:{}",
                WarmingStrategy::OnStartup {
                    priority: WarmingPriority::High,
                    batch_size: 100,
                },
                DataSource::Database {
                    query: "SELECT * FROM user_profiles LIMIT 100".to_string(),
                    parameters: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(1800)),
        ); // 30 minutes

        // Market data warming
        warmer.add_task(
            WarmingTask::new(
                "warm_market_data",
                "market:{}",
                WarmingStrategy::Scheduled {
                    interval: Duration::from_secs(300), // 5 minutes
                    priority: WarmingPriority::Critical,
                },
                DataSource::ExternalAPI {
                    url: "https://api.gridtokenx.com/market-data".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(300)),
        ); // 5 minutes

        // Order book warming
        warmer.add_task(
            WarmingTask::new(
                "warm_order_books",
                "orderbook:{}",
                WarmingStrategy::AccessPattern {
                    threshold_accesses: 10,
                    time_window: Duration::from_secs(300), // 5 minutes
                },
                DataSource::Computed {
                    function: "precompute_order_books".to_string(),
                    parameters: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(60)),
        ); // 1 minute

        // Trading statistics warming
        warmer.add_task(
            WarmingTask::new(
                "warm_trading_stats",
                "trading_stats:{}",
                WarmingStrategy::Scheduled {
                    interval: Duration::from_secs(1800), // 30 minutes
                    priority: WarmingPriority::Medium,
                },
                DataSource::Computed {
                    function: "precompute_trading_stats".to_string(),
                    parameters: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(3600)),
        ); // 1 hour

        // Price history warming
        warmer.add_task(
            WarmingTask::new(
                "warm_price_history",
                "price_history:{}",
                WarmingStrategy::OnDemand {
                    pre_fetch_factor: 1.5,
                    trigger_threshold: 5,
                },
                DataSource::Database {
                    query:
                        "SELECT * FROM price_history WHERE timestamp > NOW() - INTERVAL '1 HOUR'"
                            .to_string(),
                    parameters: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(1800)),
        ); // 30 minutes

        // Token metadata warming
        warmer.add_task(
            WarmingTask::new(
                "warm_token_metadata",
                "token_metadata:{}",
                WarmingStrategy::OnStartup {
                    priority: WarmingPriority::Medium,
                    batch_size: 50,
                },
                DataSource::Database {
                    query: "SELECT * FROM token_metadata".to_string(),
                    parameters: HashMap::new(),
                },
            )
            .with_ttl(Duration::from_secs(7200)),
        ); // 2 hours

        Ok(Self { warmer })
    }

    /// Execute all startup warming tasks
    pub async fn execute_startup_warming(&self) -> RedisResult<Vec<WarmingResult>> {
        self.warmer.execute_startup_tasks().await
    }

    /// Get warming statistics
    pub async fn get_warming_statistics(&self) -> RedisResult<WarmingStatistics> {
        self.warmer.get_warming_statistics().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warming_priority_ordering() {
        assert!(WarmingPriority::Critical < WarmingPriority::High);
        assert!(WarmingPriority::High < WarmingPriority::Medium);
        assert!(WarmingPriority::Medium < WarmingPriority::Low);
    }

    #[test]
    fn test_warming_task_creation() {
        let task = WarmingTask::new(
            "test",
            "test:{}",
            WarmingStrategy::OnStartup {
                priority: WarmingPriority::High,
                batch_size: 10,
            },
            DataSource::Static {
                data: serde_json::json!({"test": "data"}),
            },
        )
        .with_ttl(Duration::from_secs(3600));

        assert_eq!(task.id, "test");
        assert_eq!(task.ttl, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_warming_result_creation() {
        let success = WarmingResult::success("test", 10, Duration::from_secs(5));
        assert!(success.success);
        assert_eq!(success.warmed_keys, 10);

        let failure = WarmingResult::failure("test", "error", Duration::from_secs(1));
        assert!(!failure.success);
        assert_eq!(failure.error_message, Some("error".to_string()));
    }
}
