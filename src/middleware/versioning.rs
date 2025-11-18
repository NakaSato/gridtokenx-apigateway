use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
    Json,
};
use serde_json::json;

use crate::error::ApiError;

/// Supported API versions
#[derive(Debug, Clone, PartialEq)]
pub enum ApiVersion {
    V1,
}

impl ApiVersion {
    /// Parse version from string
    pub fn from_str(version: &str) -> Option<Self> {
        match version {
            "v1" | "1" => Some(ApiVersion::V1),
            _ => None,
        }
    }

    /// Get version string
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
        }
    }

    /// Get version number
    pub fn as_number(&self) -> u8 {
        match self {
            ApiVersion::V1 => 1,
        }
    }
}

/// Extract API version from request
pub fn extract_api_version(headers: &HeaderMap, path: &str) -> ApiVersion {
    // 1. Check URL path first (highest priority)
    if let Some(version_part) = extract_version_from_path(path) {
        if let Some(version) = ApiVersion::from_str(&version_part) {
            return version;
        }
    }

    // 2. Check Accept header (medium priority)
    if let Some(accept_header) = headers.get("accept") {
        if let Ok(accept_str) = accept_header.to_str() {
            if let Some(version) = extract_version_from_accept_header(accept_str) {
                return version;
            }
        }
    }

    // 3. Check custom version header (low priority)
    if let Some(version_header) = headers.get("api-version") {
        if let Ok(version_str) = version_header.to_str() {
            if let Some(version) = ApiVersion::from_str(version_str) {
                return version;
            }
        }
    }

    // 4. Default to v1 for backward compatibility
    ApiVersion::V1
}

/// Extract version from URL path
fn extract_version_from_path(path: &str) -> Option<String> {
    // Look for patterns like /api/v1/
    let path_parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    
    if path_parts.len() >= 2 && path_parts[0] == "api" {
        let potential_version = path_parts[1];
        if potential_version.starts_with('v') || potential_version.chars().next()?.is_ascii_digit() {
            return Some(potential_version.to_string());
        }
    }
    
    None
}

/// Extract version from Accept header
fn extract_version_from_accept_header(accept: &str) -> Option<ApiVersion> {
    // Look for patterns like "application/vnd.gridtokenx.v1+json"
    for part in accept.split(',') {
        let part = part.trim();
        if part.contains("application/vnd.gridtokenx.") {
            let version_start = part.find("application/vnd.gridtokenx.")? + "application/vnd.gridtokenx.".len();
            let version_end = part.find('+').unwrap_or(part.len());
            let version_part = &part[version_start..version_end];
            
            if let Some(version) = ApiVersion::from_str(version_part) {
                return Some(version);
            }
        }
    }
    None
}

/// API versioning middleware
pub async fn versioning_middleware(
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let headers = request.headers().clone();
    let path = request.uri().path();
    
    let api_version = extract_api_version(&headers, path);
    
    // Add version info to request extensions for handlers to use
    let mut request = request;
    request.extensions_mut().insert(api_version.clone());
    
    // Continue to next middleware/handler
    let mut response = next.run(request).await;
    
    // Add version headers to response
    let response_headers = response.headers_mut();
    response_headers.insert("api-version", api_version.as_str().parse().unwrap());
    response_headers.insert("api-supported-versions", "v1".parse().unwrap());
    
    Ok(response)
}

/// Middleware to check if requested version is supported
pub async fn version_check_middleware(
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let api_version = request.extensions().get::<ApiVersion>()
        .cloned()
        .unwrap_or(ApiVersion::V1); // Default to v1
    
    // Only support v1
    match api_version {
        ApiVersion::V1 => {
            // Version is supported, continue
            Ok(next.run(request).await)
        }
    }
}

/// Create version-specific response with metadata
pub fn create_versioned_response<T: serde::Serialize>(
    data: T,
    version: &ApiVersion,
) -> Json<serde_json::Value> {
    let response = json!({
        "data": data,
        "api_version": version.as_str(),
        "version": version.as_number(),
    });
    
    Json(response)
}

/// Get version info endpoint response
pub fn get_version_info() -> serde_json::Value {
    json!({
        "current_version": "v1",
        "supported_versions": ["v1"],
        "deprecated_versions": [],
        "default_version": "v1",
        "versioning_policy": {
            "url_format": "/api/{version}/endpoint",
            "header_format": "Accept: application/vnd.gridtokenx.{version}+json",
            "custom_header": "api-version: {version}"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_version_from_str() {
        assert_eq!(ApiVersion::from_str("v1"), Some(ApiVersion::V1));
        assert_eq!(ApiVersion::from_str("1"), Some(ApiVersion::V1));
        assert_eq!(ApiVersion::from_str("v2"), None);
        assert_eq!(ApiVersion::from_str("2"), None);
        assert_eq!(ApiVersion::from_str("v3"), None);
    }

    #[test]
    fn test_extract_version_from_path() {
        assert_eq!(extract_version_from_path("/api/v1/users"), Some("v1".to_string()));
        assert_eq!(extract_version_from_path("/api/v2/orders"), None); // v2 no longer supported
        assert_eq!(extract_version_from_path("/api/health"), None);
        assert_eq!(extract_version_from_path("/v1/test"), None); // Missing "api" prefix
    }

    #[test]
    fn test_extract_version_from_accept_header() {
        let accept1 = "application/vnd.gridtokenx.v1+json";
        assert_eq!(extract_version_from_accept_header(accept1), Some(ApiVersion::V1));
        
        let accept2 = "application/vnd.gridtokenx.v2+json";
        assert_eq!(extract_version_from_accept_header(accept2), None); // v2 no longer supported
        
        let accept3 = "application/json";
        assert_eq!(extract_version_from_accept_header(accept3), None);
    }
}
