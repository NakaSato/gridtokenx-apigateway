use axum::{
    extract::Request,
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Request logging middleware that logs all incoming requests and responses
pub async fn request_logger_middleware(request: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let start = Instant::now();

    // Log request
    println!("DEBUG: Request Logger Middleware Hit: {} {}", method, uri);
    info!(
        request_id = %request_id,
        method = %method,
        uri = %uri,
        "Incoming request"
    );

    // Log request headers in debug mode
    debug!(
        request_id = %request_id,
        headers = ?headers,
        "Request headers"
    );

    // Extract user info from headers if available
    if let Some(user_info) = extract_user_info(&headers) {
        debug!(
            request_id = %request_id,
            user = %user_info,
            "Authenticated user request"
        );
    }

    // Execute the request
    let response = next.run(request).await;

    let status = response.status();
    let duration = start.elapsed();

    // Log response based on status code
    match status {
        StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => {
            info!(
                request_id = %request_id,
                method = %method,
                uri = %uri,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Request completed successfully"
            );
        }
        status if status.is_client_error() => {
            warn!(
                request_id = %request_id,
                method = %method,
                uri = %uri,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Request failed with client error"
            );
        }
        status if status.is_server_error() => {
            error!(
                request_id = %request_id,
                method = %method,
                uri = %uri,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Request failed with server error"
            );
        }
        _ => {
            debug!(
                request_id = %request_id,
                method = %method,
                uri = %uri,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Request completed"
            );
        }
    }

    // Add request ID to response headers for tracing
    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert("X-Request-ID", request_id.parse().unwrap());

    Response::from_parts(parts, body)
}

/// Authentication attempt logging middleware
pub async fn auth_logger_middleware(request: Request, next: Next) -> Response {
    let uri = request.uri().clone();
    let method = request.method().clone();

    // Extract body for logging (if it's a POST request)
    let (parts, body) = request.into_parts();

    // Log authentication attempt
    info!(
        method = %method,
        uri = %uri,
        "Authentication attempt"
    );

    // Reconstruct request
    let request = Request::from_parts(parts, body);

    // Execute the request
    let response = next.run(request).await;
    let status = response.status();

    // Log authentication result
    match status {
        StatusCode::OK | StatusCode::CREATED => {
            info!(
                uri = %uri,
                status = %status,
                "Authentication successful"
            );
        }
        StatusCode::UNAUTHORIZED => {
            warn!(
                uri = %uri,
                status = %status,
                "Authentication failed - invalid credentials"
            );
        }
        StatusCode::FORBIDDEN => {
            warn!(
                uri = %uri,
                status = %status,
                "Authentication failed - access denied"
            );
        }
        StatusCode::TOO_MANY_REQUESTS => {
            warn!(
                uri = %uri,
                status = %status,
                "Authentication rate limited"
            );
        }
        status if status.is_client_error() => {
            // 400 Bad Request etc. are often validation errors, not auth errors
            debug!(
                uri = %uri,
                status = %status,
                "Request rejected (client error)"
            );
        }
        _ => {
            error!(
                uri = %uri,
                status = %status,
                "Authentication system error"
            );
        }
    }

    response
}

/// Trading activity logging middleware
pub async fn trading_logger_middleware(request: Request, next: Next) -> Response {
    let uri = request.uri().clone();
    let method = request.method().clone();
    let headers = request.headers().clone();
    let start = Instant::now();

    // Extract user info
    let user_info = extract_user_info(&headers).unwrap_or_else(|| "anonymous".to_string());

    // Log trading activity
    info!(
        method = %method,
        uri = %uri,
        user = %user_info,
        "Trading activity started"
    );

    // Execute the request
    let response = next.run(request).await;
    let status = response.status();
    let duration = start.elapsed();

    // Log trading result
    match status {
        StatusCode::OK | StatusCode::CREATED => {
            info!(
                uri = %uri,
                user = %user_info,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Trading activity completed successfully"
            );
        }
        _ => {
            warn!(
                uri = %uri,
                user = %user_info,
                status = %status,
                duration_ms = %duration.as_millis(),
                "Trading activity failed"
            );
        }
    }

    response
}

/// WebSocket connection logging middleware
pub async fn websocket_logger_middleware(request: Request, next: Next) -> Response {
    let headers = request.headers().clone();

    // Extract user info
    let user_info = extract_user_info(&headers).unwrap_or_else(|| "anonymous".to_string());

    info!(
        user = %user_info,
        "WebSocket connection attempt"
    );

    // Execute the request
    let response = next.run(request).await;
    let status = response.status();

    // Log WebSocket connection result
    match status {
        StatusCode::SWITCHING_PROTOCOLS => {
            info!(
                user = %user_info,
                "WebSocket connection established"
            );
        }
        StatusCode::UNAUTHORIZED => {
            warn!(
                user = %user_info,
                "WebSocket connection rejected - unauthorized"
            );
        }
        _ => {
            error!(
                user = %user_info,
                status = %status,
                "WebSocket connection failed"
            );
        }
    }

    response
}

/// Performance logging middleware for slow requests
pub async fn performance_logger_middleware(
    threshold_ms: u64,
    request: Request,
    next: Next,
) -> Response {
    let uri = request.uri().clone();
    let method = request.method().clone();
    let start = Instant::now();

    let response = next.run(request).await;
    let duration = start.elapsed();

    // Log if request took longer than threshold
    if duration.as_millis() > threshold_ms as u128 {
        warn!(
            method = %method,
            uri = %uri,
            duration_ms = %duration.as_millis(),
            threshold_ms = %threshold_ms,
            "Slow request detected"
        );
    }

    response
}

/// Extract user information from request headers
fn extract_user_info(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| {
            // Extract token from "Bearer <token>"
            if auth.starts_with("Bearer ") {
                Some(auth[7..].to_string())
            } else {
                None
            }
        })
        .and_then(|token| {
            // Decode JWT without verification (just to get user ID)
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Clone, Serialize, Deserialize)]
            struct Claims {
                sub: String, // user_id
                email: Option<String>,
            }

            // Try to decode without verification (just for logging)
            jsonwebtoken::dangerous::insecure_decode::<Claims>(&token)
                .ok()
                .map(|data| {
                    if let Some(email) = data.claims.email {
                        format!("{}({})", data.claims.sub, email)
                    } else {
                        data.claims.sub
                    }
                })
        })
}

/// Structured log entry for JSON logging
#[derive(serde::Serialize)]
pub struct StructuredLogEntry {
    pub timestamp: String,
    pub request_id: String,
    pub method: String,
    pub uri: String,
    pub status: u16,
    pub duration_ms: u128,
    pub user_id: Option<String>,
    pub ip_address: Option<String>,
}

impl StructuredLogEntry {
    pub fn new(
        request_id: String,
        method: Method,
        uri: String,
        status: StatusCode,
        duration_ms: u128,
        headers: &HeaderMap,
    ) -> Self {
        let user_id = extract_user_info(headers);
        let ip_address = headers
            .get("x-forwarded-for")
            .or_else(|| headers.get("x-real-ip"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id,
            method: method.to_string(),
            uri,
            status: status.as_u16(),
            duration_ms,
            user_id,
            ip_address,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_user_info() {
        let mut headers = HeaderMap::new();

        // Test without authorization header
        assert_eq!(extract_user_info(&headers), None);

        // Test with invalid authorization header
        headers.insert("authorization", HeaderValue::from_static("InvalidToken"));
        assert_eq!(extract_user_info(&headers), None);
    }

    #[test]
    fn test_structured_log_entry() {
        let headers = HeaderMap::new();
        let entry = StructuredLogEntry::new(
            "test-id".to_string(),
            Method::GET,
            "/api/test".to_string(),
            StatusCode::OK,
            100,
            &headers,
        );

        assert_eq!(entry.request_id, "test-id");
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.uri, "/api/test");
        assert_eq!(entry.status, 200);
        assert_eq!(entry.duration_ms, 100);
    }
}
