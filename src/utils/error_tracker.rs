use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use crate::error::ErrorCode;

/// Error tracking metrics
#[derive(Debug, Clone, Serialize)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub errors_by_code: HashMap<String, u64>,
    pub errors_by_endpoint: HashMap<String, u64>,
    pub last_errors: Vec<ErrorEntry>,
}

/// Individual error entry for tracking
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEntry {
    pub timestamp: DateTime<Utc>,
    pub error_code: ErrorCode,
    pub endpoint: String,
    pub user_id: Option<String>,
    pub message: String,
    pub request_id: String,
}

/// Error tracker service
#[derive(Clone)]
pub struct ErrorTracker {
    metrics: Arc<RwLock<ErrorMetrics>>,
    max_last_errors: usize,
}

impl ErrorTracker {
    /// Create a new error tracker
    pub fn new(max_last_errors: usize) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(ErrorMetrics {
                total_errors: 0,
                errors_by_code: HashMap::new(),
                errors_by_endpoint: HashMap::new(),
                last_errors: Vec::new(),
            })),
            max_last_errors,
        }
    }

    /// Track an error
    pub async fn track_error(
        &self,
        error_code: ErrorCode,
        endpoint: String,
        user_id: Option<String>,
        message: String,
        request_id: String,
    ) {
        let mut metrics = self.metrics.write().await;
        
        // Increment total errors
        metrics.total_errors += 1;
        
        // Track by error code
        let code_str = format!("{:?}", error_code);
        *metrics.errors_by_code.entry(code_str).or_insert(0) += 1;
        
        // Track by endpoint
        *metrics.errors_by_endpoint.entry(endpoint.clone()).or_insert(0) += 1;
        
        // Add to last errors (keep only last N)
        let entry = ErrorEntry {
            timestamp: Utc::now(),
            error_code,
            endpoint,
            user_id,
            message,
            request_id,
        };
        
        metrics.last_errors.push(entry);
        
        // Keep only the last N errors
        if metrics.last_errors.len() > self.max_last_errors {
            metrics.last_errors.remove(0);
        }
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> ErrorMetrics {
        self.metrics.read().await.clone()
    }

    /// Get error rate for specific endpoint
    pub async fn get_endpoint_error_rate(&self, endpoint: &str) -> u64 {
        self.metrics
            .read()
            .await
            .errors_by_endpoint
            .get(endpoint)
            .copied()
            .unwrap_or(0)
    }

    /// Get top error codes
    pub async fn get_top_error_codes(&self, limit: usize) -> Vec<(String, u64)> {
        let metrics = self.metrics.read().await;
        let mut codes: Vec<_> = metrics.errors_by_code.iter().collect();
        codes.sort_by(|a, b| b.1.cmp(a.1));
        codes
            .into_iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// Get top error endpoints
    pub async fn get_top_error_endpoints(&self, limit: usize) -> Vec<(String, u64)> {
        let metrics = self.metrics.read().await;
        let mut endpoints: Vec<_> = metrics.errors_by_endpoint.iter().collect();
        endpoints.sort_by(|a, b| b.1.cmp(a.1));
        endpoints
            .into_iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.total_errors = 0;
        metrics.errors_by_code.clear();
        metrics.errors_by_endpoint.clear();
        metrics.last_errors.clear();
    }

    /// Get recent errors
    pub async fn get_recent_errors(&self, limit: usize) -> Vec<ErrorEntry> {
        let metrics = self.metrics.read().await;
        let start = if metrics.last_errors.len() > limit {
            metrics.last_errors.len() - limit
        } else {
            0
        };
        metrics.last_errors[start..].to_vec()
    }
}

/// Global error tracker instance
static ERROR_TRACKER: once_cell::sync::Lazy<ErrorTracker> =
    once_cell::sync::Lazy::new(|| ErrorTracker::new(100));

/// Get the global error tracker
pub fn get_error_tracker() -> &'static ErrorTracker {
    &ERROR_TRACKER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_tracking() {
        let tracker = ErrorTracker::new(10);

        // Track some errors
        tracker
            .track_error(
                ErrorCode::InvalidCredentials,
                "/api/auth/login".to_string(),
                Some("user123".to_string()),
                "Invalid password".to_string(),
                "req-123".to_string(),
            )
            .await;

        tracker
            .track_error(
                ErrorCode::NotFound,
                "/api/users/999".to_string(),
                Some("user456".to_string()),
                "User not found".to_string(),
                "req-456".to_string(),
            )
            .await;

        // Check metrics
        let metrics = tracker.get_metrics().await;
        assert_eq!(metrics.total_errors, 2);
        assert_eq!(metrics.last_errors.len(), 2);
    }

    #[tokio::test]
    async fn test_max_last_errors() {
        let tracker = ErrorTracker::new(5);

        // Track 10 errors
        for i in 0..10 {
            tracker
                .track_error(
                    ErrorCode::InvalidInput,
                    format!("/api/endpoint/{}", i),
                    None,
                    format!("Error {}", i),
                    format!("req-{}", i),
                )
                .await;
        }

        // Should only keep last 5
        let metrics = tracker.get_metrics().await;
        assert_eq!(metrics.last_errors.len(), 5);
        assert_eq!(metrics.total_errors, 10);
    }

    #[tokio::test]
    async fn test_top_error_codes() {
        let tracker = ErrorTracker::new(100);

        // Track multiple errors
        for _ in 0..5 {
            tracker
                .track_error(
                    ErrorCode::InvalidCredentials,
                    "/api/auth/login".to_string(),
                    None,
                    "Error".to_string(),
                    "req".to_string(),
                )
                .await;
        }

        for _ in 0..3 {
            tracker
                .track_error(
                    ErrorCode::NotFound,
                    "/api/users".to_string(),
                    None,
                    "Error".to_string(),
                    "req".to_string(),
                )
                .await;
        }

        let top_codes = tracker.get_top_error_codes(2).await;
        assert_eq!(top_codes.len(), 2);
        assert_eq!(top_codes[0].1, 5); // InvalidCredentials should be first
    }
}
