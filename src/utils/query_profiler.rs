//! Query Profiler Utility
//!
//! Provides utilities for profiling database queries and detecting slow queries.
//! Use this module to wrap database operations and log performance metrics.

use std::future::Future;
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};

/// Default threshold for logging slow queries (in milliseconds)
const SLOW_QUERY_THRESHOLD_MS: u64 = 100;

/// Critical threshold for very slow queries (in milliseconds)
const CRITICAL_QUERY_THRESHOLD_MS: u64 = 1000;

/// Query profiling result with timing information
#[derive(Debug, Clone)]
pub struct QueryProfile {
    /// Name/description of the query
    pub query_name: String,
    /// Duration of the query
    pub duration: Duration,
    /// Whether the query was successful
    pub success: bool,
    /// Row count if applicable
    pub row_count: Option<usize>,
}

impl QueryProfile {
    /// Check if this query is considered slow
    pub fn is_slow(&self) -> bool {
        self.duration.as_millis() > SLOW_QUERY_THRESHOLD_MS.into()
    }

    /// Check if this query is critically slow
    pub fn is_critical(&self) -> bool {
        self.duration.as_millis() > CRITICAL_QUERY_THRESHOLD_MS.into()
    }
}

/// Profile a database query and log timing information
///
/// # Example
/// ```rust,ignore
/// use crate::utils::query_profiler::profile_query;
///
/// let users = profile_query("fetch_all_users", async {
///     sqlx::query!("SELECT * FROM users").fetch_all(&pool).await
/// }).await?;
/// ```
pub async fn profile_query<F, T, E>(query_name: &str, query: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let start = Instant::now();
    let result = query.await;
    let duration = start.elapsed();

    let success = result.is_ok();
    let profile = QueryProfile {
        query_name: query_name.to_string(),
        duration,
        success,
        row_count: None,
    };

    log_query_profile(&profile);

    result
}

/// Profile a query that returns a collection and track row count
pub async fn profile_query_with_count<F, T, E, C>(
    query_name: &str,
    query: F,
    extract_count: C,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
    C: Fn(&T) -> usize,
{
    let start = Instant::now();
    let result = query.await;
    let duration = start.elapsed();

    let (success, row_count) = match &result {
        Ok(data) => (true, Some(extract_count(data))),
        Err(_) => (false, None),
    };

    let profile = QueryProfile {
        query_name: query_name.to_string(),
        duration,
        success,
        row_count,
    };

    log_query_profile(&profile);

    result
}

/// Log query profile with appropriate log level based on duration
fn log_query_profile(profile: &QueryProfile) {
    let duration_ms = profile.duration.as_millis();

    if !profile.success {
        error!(
            query = %profile.query_name,
            duration_ms = duration_ms,
            "Query failed"
        );
    } else if profile.is_critical() {
        error!(
            query = %profile.query_name,
            duration_ms = duration_ms,
            row_count = ?profile.row_count,
            "🚨 CRITICAL: Query extremely slow (>{}ms)", CRITICAL_QUERY_THRESHOLD_MS
        );
    } else if profile.is_slow() {
        warn!(
            query = %profile.query_name,
            duration_ms = duration_ms,
            row_count = ?profile.row_count,
            "⚠️ SLOW QUERY: Query exceeded threshold (>{}ms)", SLOW_QUERY_THRESHOLD_MS
        );
    } else {
        debug!(
            query = %profile.query_name,
            duration_ms = duration_ms,
            row_count = ?profile.row_count,
            "Query completed"
        );
    }
}

/// Query batcher for optimizing multiple related queries
pub struct QueryBatcher {
    profiles: Vec<QueryProfile>,
    batch_name: String,
    start_time: Instant,
}

impl QueryBatcher {
    /// Create a new query batcher
    pub fn new(batch_name: &str) -> Self {
        Self {
            profiles: Vec::new(),
            batch_name: batch_name.to_string(),
            start_time: Instant::now(),
        }
    }

    /// Record a query profile
    pub fn record(&mut self, profile: QueryProfile) {
        self.profiles.push(profile);
    }

    /// Profile a query within this batch
    pub async fn profile<F, T, E>(&mut self, query_name: &str, query: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let result = query.await;
        let duration = start.elapsed();

        let profile = QueryProfile {
            query_name: query_name.to_string(),
            duration,
            success: result.is_ok(),
            row_count: None,
        };

        self.profiles.push(profile);
        result
    }

    /// Finish the batch and log summary
    pub fn finish(self) {
        let total_duration = self.start_time.elapsed();
        let total_queries = self.profiles.len();
        let slow_queries = self.profiles.iter().filter(|p| p.is_slow()).count();
        let failed_queries = self.profiles.iter().filter(|p| !p.success).count();

        if failed_queries > 0 {
            warn!(
                batch = %self.batch_name,
                total_queries = total_queries,
                slow_queries = slow_queries,
                failed_queries = failed_queries,
                total_duration_ms = total_duration.as_millis(),
                "Query batch completed with failures"
            );
        } else if slow_queries > 0 {
            warn!(
                batch = %self.batch_name,
                total_queries = total_queries,
                slow_queries = slow_queries,
                total_duration_ms = total_duration.as_millis(),
                "Query batch completed with slow queries"
            );
        } else {
            info!(
                batch = %self.batch_name,
                total_queries = total_queries,
                total_duration_ms = total_duration.as_millis(),
                "Query batch completed successfully"
            );
        }
    }
}

/// Create a simple timer for manual profiling
pub struct QueryTimer {
    name: String,
    start: Instant,
}

impl QueryTimer {
    /// Start a new timer
    pub fn start(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: Instant::now(),
        }
    }

    /// Stop the timer and log the duration
    pub fn stop(self) -> Duration {
        let duration = self.start.elapsed();
        let duration_ms = duration.as_millis();

        if duration_ms > CRITICAL_QUERY_THRESHOLD_MS.into() {
            error!(
                query = %self.name,
                duration_ms = duration_ms,
                "🚨 CRITICAL: Operation extremely slow"
            );
        } else if duration_ms > SLOW_QUERY_THRESHOLD_MS.into() {
            warn!(
                query = %self.name,
                duration_ms = duration_ms,
                "⚠️ SLOW: Operation exceeded threshold"
            );
        } else {
            debug!(
                query = %self.name,
                duration_ms = duration_ms,
                "Operation completed"
            );
        }

        duration
    }

    /// Stop the timer and return duration without logging
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_query_profiler() {
        let result: Result<i32, &str> = profile_query("test_query", async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(42)
        }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_query_profile_thresholds() {
        let slow_profile = QueryProfile {
            query_name: "slow".to_string(),
            duration: Duration::from_millis(150),
            success: true,
            row_count: None,
        };

        let fast_profile = QueryProfile {
            query_name: "fast".to_string(),
            duration: Duration::from_millis(50),
            success: true,
            row_count: None,
        };

        assert!(slow_profile.is_slow());
        assert!(!slow_profile.is_critical());
        assert!(!fast_profile.is_slow());
    }

    #[test]
    fn test_query_timer() {
        let timer = QueryTimer::start("test_timer");
        std::thread::sleep(Duration::from_millis(10));
        let duration = timer.stop();

        assert!(duration >= Duration::from_millis(10));
    }
}
