use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use metrics::{counter, gauge, histogram};
use std::time::Instant;

/// Metrics middleware that tracks request metrics
pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    // Increment request counter
    counter!("http_requests_total", "method" => method.to_string(), "path" => path.clone()).increment(1);

    // Execute request
    let response = next.run(request).await;
    
    let status = response.status();
    let duration = start.elapsed();

    // Record request duration
    histogram!(
        "http_request_duration_seconds",
        "method" => method.to_string(),
        "path" => path.clone(),
        "status" => status.as_u16().to_string()
    ).record(duration.as_secs_f64());

    // Track active requests
    gauge!("http_requests_in_flight", "path" => path.clone()).increment(-1.0);

    // Track status codes
    counter!(
        "http_responses_total",
        "method" => method.to_string(),
        "path" => path.clone(),
        "status" => status.as_u16().to_string()
    ).increment(1);

    // Track errors
    if status.is_server_error() {
        counter!(
            "http_errors_total",
            "method" => method.to_string(),
            "path" => path.clone(),
            "status" => status.as_u16().to_string()
        ).increment(1);
    }

    response
}

/// Middleware to track active requests
pub async fn active_requests_middleware(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    
    // Increment active requests
    gauge!("http_requests_in_flight", "path" => path.clone()).increment(1.0);

    let response = next.run(request).await;
    
    // Decrement active requests (done in metrics_middleware)
    response
}

/// Track authentication attempts
pub fn track_auth_attempt(success: bool, method: &str) {
    counter!(
        "auth_attempts_total",
        "method" => method.to_string(),
        "success" => success.to_string()
    ).increment(1);
}

/// Track authentication failures
pub fn track_auth_failure(reason: &str) {
    counter!("auth_failures_total", "reason" => reason.to_string()).increment(1);
}

/// Track trading operations
pub fn track_trading_operation(operation: &str, success: bool) {
    counter!(
        "trading_operations_total",
        "operation" => operation.to_string(),
        "success" => success.to_string()
    ).increment(1);
}

/// Track order creation
pub fn track_order_created(order_type: &str) {
    counter!("orders_created_total", "type" => order_type.to_string()).increment(1);
}

/// Track order matching
pub fn track_order_matched(order_type: &str, amount: f64) {
    counter!("orders_matched_total", "type" => order_type.to_string()).increment(1);
    histogram!("order_match_amount", "type" => order_type.to_string()).record(amount);
}

/// Track WebSocket connections
pub fn track_websocket_connection(connected: bool) {
    if connected {
        gauge!("websocket_connections_active").increment(1.0);
    } else {
        gauge!("websocket_connections_active").decrement(1.0);
    }
}

/// Track database operations
pub fn track_database_operation(operation: &str, duration_ms: f64, success: bool) {
    histogram!(
        "database_operation_duration_ms",
        "operation" => operation.to_string(),
        "success" => success.to_string()
    ).record(duration_ms);
    
    counter!(
        "database_operations_total",
        "operation" => operation.to_string(),
        "success" => success.to_string()
    ).increment(1);
}

/// Track blockchain operations
pub fn track_blockchain_operation(operation: &str, duration_ms: f64, success: bool) {
    histogram!(
        "blockchain_operation_duration_ms",
        "operation" => operation.to_string(),
        "success" => success.to_string()
    ).record(duration_ms);
    
    counter!(
        "blockchain_operations_total",
        "operation" => operation.to_string(),
        "success" => success.to_string()
    ).increment(1);
}

/// Track cache operations
pub fn track_cache_operation(operation: &str, hit: bool) {
    counter!(
        "cache_operations_total",
        "operation" => operation.to_string(),
        "result" => if hit { "hit" } else { "miss" }
    ).increment(1);
}

/// Track token minting
pub fn track_token_mint(amount: f64, success: bool) {
    counter!("tokens_minted_total", "success" => success.to_string()).increment(1);
    if success {
        histogram!("token_mint_amount").record(amount);
    }
}

/// Track meter readings
pub fn track_meter_reading(success: bool) {
    counter!("meter_readings_total", "success" => success.to_string()).increment(1);
}

/// Track API rate limits
pub fn track_rate_limit_hit(endpoint: &str) {
    counter!("rate_limit_hits_total", "endpoint" => endpoint.to_string()).increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_auth_attempt() {
        track_auth_attempt(true, "password");
        track_auth_attempt(false, "wallet");
        // Metrics are recorded successfully
    }

    #[test]
    fn test_track_trading_operation() {
        track_trading_operation("create_order", true);
        track_trading_operation("cancel_order", false);
    }

    #[test]
    fn test_track_websocket_connection() {
        track_websocket_connection(true);
        track_websocket_connection(false);
    }
}
