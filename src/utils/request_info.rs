// Utility functions for extracting request information for audit logging

use axum::http::HeaderMap;

/// Extract IP address from request headers
/// Checks X-Forwarded-For, X-Real-IP headers for proxied requests
pub fn extract_ip_address(headers: &HeaderMap) -> String {
    // Check X-Forwarded-For header first (for proxied requests)
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            // X-Forwarded-For can contain multiple IPs, take the first one
            let ip = value.split(',').next().unwrap_or("").trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }

    // Check X-Real-IP header (common in nginx)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }

    // Fallback to localhost if no IP headers found (valid inet format for database)
    "127.0.0.1".to_string()
}

/// Extract User-Agent from request headers
pub fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.1, 198.51.100.1"));
        
        assert_eq!(extract_ip_address(&headers), "203.0.113.1");
    }

    #[test]
    fn test_extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("192.168.1.1"));
        
        assert_eq!(extract_ip_address(&headers), "192.168.1.1");
    }

    #[test]
    fn test_extract_ip_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.1"));
        headers.insert("x-real-ip", HeaderValue::from_static("192.168.1.1"));
        
        // X-Forwarded-For should take priority
        assert_eq!(extract_ip_address(&headers), "203.0.113.1");
    }

    #[test]
    fn test_extract_ip_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ip_address(&headers), "127.0.0.1");
    }

    #[test]
    fn test_extract_user_agent() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"));
        
        assert_eq!(extract_user_agent(&headers), Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)".to_string()));
    }

    #[test]
    fn test_extract_user_agent_none() {
        let headers = HeaderMap::new();
        assert_eq!(extract_user_agent(&headers), None);
    }
}
