use axum::{
    body::Body,
    http::{header, Request, Response},
    middleware::Next,
};

/// Add security headers to all responses to prevent common web vulnerabilities
///
/// Headers added:
/// - X-Content-Type-Options: nosniff (prevent MIME sniffing)
/// - X-Frame-Options: DENY (prevent clickjacking)
/// - X-XSS-Protection: 1; mode=block (XSS protection)
/// - Content-Security-Policy: Restrict resource loading
/// - Referrer-Policy: Control referrer information
/// - Permissions-Policy: Restrict feature access
pub async fn add_security_headers(
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    
    // Prevent MIME type sniffing
    // Protects against: Drive-by downloads, MIME confusion attacks
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("Failed to parse nosniff header value")
    );
    
    // Prevent clickjacking attacks
    // Protects against: UI redressing, clickjacking
    headers.insert(
        header::X_FRAME_OPTIONS,
        "DENY".parse().expect("Failed to parse DENY header value")
    );
    
    // Enable XSS protection (legacy but still useful for older browsers)
    // Protects against: Cross-site scripting
    headers.insert(
        header::HeaderName::from_static("x-xss-protection"),
        "1; mode=block".parse().expect("Failed to parse XSS protection header value")
    );
    
    // Content Security Policy - restrict resource loading
    // Protects against: XSS, data injection attacks
    let csp = "default-src 'self'; \
               script-src 'self' 'unsafe-inline'; \
               style-src 'self' 'unsafe-inline'; \
               img-src 'self' data: https:; \
               font-src 'self' data:; \
               connect-src 'self'; \
               frame-ancestors 'none'; \
               base-uri 'self'; \
               form-action 'self'";
    
    headers.insert(
        header::HeaderName::from_static("content-security-policy"),
        csp.parse().expect("Failed to parse CSP header value")
    );
    
    // Control referrer information sent to external sites
    // Protects against: Information leakage
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        "strict-origin-when-cross-origin".parse().expect("Failed to parse referrer policy header value")
    );
    
    // Restrict browser features and APIs
    // Protects against: Unwanted feature access
    let permissions = "geolocation=(), \
                      microphone=(), \
                      camera=(), \
                      payment=(), \
                      usb=(), \
                      magnetometer=(), \
                      gyroscope=(), \
                      accelerometer=()";
    
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        permissions.parse().expect("Failed to parse permissions policy header value")
    );
    
    // Remove server identification (if present)
    headers.remove(header::SERVER);
    
    // Add custom security header for API version (helps with incident response)
    headers.insert(
        header::HeaderName::from_static("x-api-version"),
        "1.0".parse().expect("Failed to parse API version header value")
    );
    
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        response::IntoResponse,
        Router,
        routing::get,
    };
    use tower::ServiceExt;

    async fn test_handler() -> impl IntoResponse {
        (StatusCode::OK, "test response")
    }

    #[tokio::test]
    async fn test_security_headers_added() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(from_fn(add_security_headers));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let headers = response.headers();

        // Verify all security headers are present
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        assert_eq!(
            headers.get(header::X_FRAME_OPTIONS).unwrap(),
            "DENY"
        );
        assert_eq!(
            headers.get("X-XSS-Protection").unwrap(),
            "1; mode=block"
        );
        assert!(headers.contains_key("Content-Security-Policy"));
        assert_eq!(
            headers.get("Referrer-Policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert!(headers.contains_key("Permissions-Policy"));
        assert_eq!(
            headers.get("X-API-Version").unwrap(),
            "1.0"
        );
        
        // Verify server header is removed
        assert!(!headers.contains_key(header::SERVER));
    }

    #[tokio::test]
    async fn test_csp_header_content() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(from_fn(add_security_headers));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let csp = response.headers()
            .get("Content-Security-Policy")
            .unwrap()
            .to_str()
            .unwrap();

        // Verify CSP contains important directives
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("base-uri 'self'"));
    }
}
