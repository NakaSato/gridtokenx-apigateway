use redis::{AsyncCommands, Client, RedisError, RedisResult};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

use super::simulation::WarmingSimulator;
use super::types::{
    DataSource, RetryConfig, WarmingPriority, WarmingResult, WarmingStatistics, WarmingStrategy,
    WarmingTask,
};

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
            max_history_size: 100,
        })
    }

    /// Add a warming task
    pub fn add_task(&mut self, task: WarmingTask) {
        info!("Adding cache warming task: {}", task.id);
        self.tasks.insert(task.id.clone(), task);
    }

    /// Execute a specific warming task
    pub async fn execute_task(&mut self, task_id: &str) -> RedisResult<WarmingResult> {
        let task = self.tasks.get(task_id).ok_or_else(|| {
            RedisError::from((
                redis::ErrorKind::TypeError,
                "Task not found",
                format!("Task {} not found", task_id),
            ))
        })?;

        // Clone task to avoid borrow checker issues
        let task = task.clone();

        info!("Executing warming task: {}", task.id);
        let start_time = SystemTime::now();

        // Check dependencies
        for dep in &task.dependencies {
            if !self.check_dependency(dep).await? {
                warn!("Dependency {} not satisfied for task {}", dep, task.id);
                let result = WarmingResult::failure(
                    &task.id,
                    &format!("Dependency {} not satisfied", dep),
                    start_time.elapsed().unwrap_or_default(),
                );
                self.record_execution(result.clone()).await;
                return Ok(result);
            }
        }

        let result = match &task.data_source {
            DataSource::Database { query, parameters } => {
                self.warm_from_database(&task, query, parameters).await
            }
            DataSource::ExternalAPI {
                url,
                method,
                headers,
            } => {
                self.warm_from_external_api(&task, url, method, headers)
                    .await
            }
            DataSource::Computed {
                function,
                parameters,
            } => {
                self.warm_from_computed_data(&task, function, parameters)
                    .await
            }
            DataSource::Static { data } => self.warm_from_static_data(&task, data).await,
            DataSource::Combined { .. } => {
                // Implementation for combined sources would go here
                Ok(WarmingResult::success(
                    &task.id,
                    0,
                    start_time.elapsed().unwrap_or_default(),
                ))
            }
        };

        match result {
            Ok(res) => {
                info!(
                    "Warming task {} completed: {} keys warmed in {:?}",
                    task.id, res.warmed_keys, res.duration
                );
                self.record_execution(res.clone()).await;
                Ok(res)
            }
            Err(e) => {
                error!("Warming task {} failed: {}", task.id, e);
                let failure_res = WarmingResult::failure(
                    &task.id,
                    &e.to_string(),
                    start_time.elapsed().unwrap_or_default(),
                );
                self.record_execution(failure_res.clone()).await;
                Err(e)
            }
        }
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
        let value_str = serde_json::to_string(value).unwrap_or_default();

        let _: () = conn.set(&cache_key, &value_str).await?;

        if let Some(duration) = ttl {
            let _: () = conn.expire(&cache_key, duration.as_secs() as i64).await?;
        }

        Ok(())
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
        let mut _failed_keys = 0u32;

        debug!("Warming from database: {}", query);

        // Simulate database query using helper
        let db_data = WarmingSimulator::simulate_database_query(query, parameters).await?;

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        for (key, value) in db_data {
            match self
                .set_cache_value(&mut conn, &task.key_pattern, &key, &value, task.ttl)
                .await
            {
                Ok(_) => {
                    warmed_keys += 1;
                    debug!("Warmed cache key: {}", key);
                }
                Err(e) => {
                    _failed_keys += 1;
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
        let mut _failed_keys = 0u32;

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
                    _failed_keys += 1;
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
        let mut _failed_keys = 0u32;

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
                        _failed_keys += 1;
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
        let mut _failed_keys = 0u32;

        // Execute function to generate data using helper
        let computed_data = WarmingSimulator::execute_computation(function, parameters).await?;

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
                    _failed_keys += 1;
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
            let _: RedisResult<()> = conn.set(&key, &value).await;
            let _: RedisResult<()> = conn.expire(&key, 86400).await; // 24 hours
        }

        // Mark dependencies as satisfied
        if result.success {
            let dep_key = format!("warming_dependency:{}", result.task_id);
            if let Ok(mut conn) = self.client.get_multiplexed_async_connection().await {
                let _: RedisResult<()> = conn.set(&dep_key, "completed").await;
                let _: RedisResult<()> = conn.expire(&dep_key, 3600).await; // 1 hour
            }
        }
    }

    /// Execute all startup tasks
    pub async fn execute_startup_tasks(&mut self) -> RedisResult<Vec<WarmingResult>> {
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

        let task_ids: Vec<String> = sorted_tasks.iter().map(|t| t.id.clone()).collect();

        let mut results = Vec::new();
        for task_id in task_ids {
            let result = self.execute_task(&task_id).await?;
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

    /// Fetch data from external API with retry logic
    async fn fetch_with_retry(
        &self,
        url: &str,
        method: &str,
        _headers: &HashMap<String, String>,
        retry_config: &RetryConfig,
    ) -> RedisResult<HashMap<String, serde_json::Value>> {
        debug!("Fetching from API: {} {}", method, url);

        let mut delay = retry_config.base_delay;

        for attempt in 0..=retry_config.max_retries {
            // In a real implementation, this would make actual HTTP requests
            // For this example, we'll simulate API data using helper
            let simulated_data = WarmingSimulator::simulate_api_response(url).await;

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
    pub async fn execute_startup_warming(&mut self) -> RedisResult<Vec<WarmingResult>> {
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
