// Metrics middleware for collecting API request metrics
// This middleware records request duration, status codes, and other metrics

use axum::{
    extract::Request,
    http::{HeaderValue, Uri},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::services::transaction_metrics::ApiMetrics;

/// Request ID header name
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Metrics middleware to collect API request metrics
pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let start_time = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();

    // Generate or extract request ID
    let request_id = extract_or_generate_request_id(&request);

    // Extract endpoint path without query parameters
    let path = extract_path_from_uri(&uri);

    // Log the incoming request
    debug!("Request started: {} {} (ID: {})", method, path, request_id);

    // Process the request
    let response = next.run(request).await;

    // Calculate request duration
    let duration = start_time.elapsed();
    let duration_seconds = duration.as_secs_f64();

    // Get status code
    let status = response.status();
    let status_code = status.as_u16();

    // Record metrics
    ApiMetrics::record_request(&path, method.as_str(), status_code, duration_seconds);

    // Log the completed request
    debug!(
        "Request completed: {} {} (ID: {}, Status: {}, Duration: {}ms)",
        method,
        path,
        request_id,
        status_code,
        duration.as_millis()
    );

    // Record slow requests as warnings
    if duration.as_millis() > 1000 {
        warn!(
            "Slow request detected: {} {} (ID: {}, Duration: {}ms)",
            method,
            path,
            request_id,
            duration.as_millis()
        );
    }

    // Record error requests with additional context
    if status_code >= 400 {
        error!(
            "Error response: {} {} (ID: {}, Status: {}, Duration: {}ms)",
            method,
            path,
            request_id,
            status_code,
            duration.as_millis()
        );
    }

    // Add request ID to response headers
    let (mut parts, body) = response.into_parts();
    parts.headers.insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id).unwrap_or_else(|_| {
            error!("Failed to add request ID header");
            HeaderValue::from_static("unknown")
        }),
    );

    Response::from_parts(parts, body)
}

/// Extract existing request ID or generate a new one
fn extract_or_generate_request_id<B>(request: &Request<B>) -> String {
    // Try to extract from headers
    if let Some(header_value) = request.headers().get(REQUEST_ID_HEADER) {
        if let Ok(header_str) = header_value.to_str() {
            return header_str.to_string();
        }
    }

    // Generate a new UUID as request ID
    Uuid::new_v4().to_string()
}

/// Extract path from URI without query parameters
fn extract_path_from_uri(uri: &Uri) -> String {
    let path_and_query = uri.path_and_query();

    match path_and_query {
        Some(pq) => pq.path().to_string(),
        None => "/".to_string(),
    }
}

/// Helper macro to instrument handlers with metrics
#[macro_export]
macro_rules! instrument_handler {
    ($handler:expr) => {{
        use axum::middleware;

        middleware::from_fn(metrics_middleware).layer($handler)
    }};
}

/// Helper function to normalize endpoint paths for metrics
/// Removes IDs from paths to aggregate metrics for similar endpoints
pub fn normalize_path_for_metrics(path: &str) -> String {
    // Define path patterns to normalize
    let patterns = [
        // Transaction IDs
        (
            r"^/api/v1/transactions/[0-9a-f-]+/status",
            "/api/v1/transactions/{id}/status",
        ),
        (
            r"^/api/v1/transactions/[0-9a-f-]+/retry",
            "/api/v1/transactions/{id}/retry",
        ),
        // Order IDs
        (
            r"^/api/v1/trading/orders/[0-9a-f-]+",
            "/api/v1/trading/orders/{id}",
        ),
        // Meter IDs
        (r"^/api/v1/meters/[0-9a-f-]+", "/api/v1/meters/{id}"),
        (
            r"^/api/v1/meters/[0-9a-f-]+/readings",
            "/api/v1/meters/{id}/readings",
        ),
        // Settlement IDs
        (
            r"^/api/v1/settlements/[0-9a-f-]+",
            "/api/v1/settlements/{id}",
        ),
        // User IDs
        (r"^/api/v1/users/[0-9a-f-]+", "/api/v1/users/{id}"),
    ];

    let mut normalized_path = path.to_string();

    for (pattern, replacement) in patterns {
        if let Ok(regex) = regex::Regex::new(pattern) {
            normalized_path = regex.replace(&normalized_path, replacement).to_string();
        }
    }

    normalized_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_for_metrics() {
        assert_eq!(
            normalize_path_for_metrics(
                "/api/v1/transactions/123e4567-e89b-12d3-a456-426614174000/status"
            ),
            "/api/v1/transactions/{id}/status"
        );

        assert_eq!(
            normalize_path_for_metrics(
                "/api/v1/transactions/123e4567-e89b-12d3-a456-426614174000/retry"
            ),
            "/api/v1/transactions/{id}/retry"
        );

        assert_eq!(
            normalize_path_for_metrics(
                "/api/v1/trading/orders/123e4567-e89b-12d3-a456-426614174000"
            ),
            "/api/v1/trading/orders/{id}"
        );

        assert_eq!(
            normalize_path_for_metrics("/api/v1/health"),
            "/api/v1/health"
        );
    }

    #[test]
    fn test_extract_path_from_uri() {
        use axum::http::Uri;

        let uri_with_query = Uri::from_static("/api/v1/transactions?limit=10&status=confirmed");
        assert_eq!(
            extract_path_from_uri(&uri_with_query),
            "/api/v1/transactions"
        );

        let uri_without_query = Uri::from_static("/api/v1/transactions/history");
        assert_eq!(
            extract_path_from_uri(&uri_without_query),
            "/api/v1/transactions/history"
        );
    }
}
