use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::{ToSchema, IntoParams};
use uuid::Uuid;
use std::net::IpAddr;
use sqlx::types::ipnetwork::IpNetwork;

use crate::{
    auth::middleware::AuthenticatedUser,
    error::ApiError,
    services::meter_verification_service::{
        VerifyMeterRequest, VerifyMeterResponse, 
        MeterRegistry, VerificationStats
    },
    AppState,
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyMeterRequestWrapper {
    #[serde(flatten)]
    pub request: VerifyMeterRequest,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterRegistryResponse {
    pub id: Uuid,
    pub meter_serial: String,
    pub verification_method: String,
    pub verification_status: String,
    pub user_id: Uuid,
    pub manufacturer: Option<String>,
    pub meter_type: Option<String>,
    pub location_address: Option<String>,
    pub installation_date: Option<chrono::NaiveDate>,
    pub verification_proof: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<MeterRegistry> for MeterRegistryResponse {
    fn from(meter: MeterRegistry) -> Self {
        Self {
            id: meter.id,
            meter_serial: meter.meter_serial,
            verification_method: meter.verification_method,
            verification_status: meter.verification_status,
            user_id: meter.user_id,
            manufacturer: meter.manufacturer,
            meter_type: meter.meter_type,
            location_address: meter.location_address,
            installation_date: meter.installation_date,
            verification_proof: meter.verification_proof,
            verified_at: meter.verified_at,
            verified_by: meter.verified_by,
            created_at: meter.created_at,
            updated_at: meter.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetMetersResponse {
    pub meters: Vec<MeterRegistryResponse>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct GetMetersQuery {
    /// Filter by verification status: "verified", "pending", "rejected", "suspended"
    pub status: Option<String>,
    /// Filter by meter type: "residential", "commercial", "solar", "industrial"
    pub meter_type: Option<String>,
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Number of items per page (max 100)
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

impl GetMetersQuery {
    pub fn validate(&mut self) -> Result<(), ApiError> {
        if self.page < 1 {
            self.page = 1;
        }
        
        if self.page_size < 1 {
            self.page_size = 20;
        } else if self.page_size > 100 {
            self.page_size = 100;
        }
        
        // Validate status filter
        if let Some(status) = &self.status {
            match status.as_str() {
                "verified" | "pending" | "rejected" | "suspended" => {}
                _ => return Err(ApiError::validation_error(
                    "Invalid status filter. Allowed values: verified, pending, rejected, suspended",
                    Some("status"),
                )),
            }
        }
        
        // Validate meter type filter
        if let Some(meter_type) = &self.meter_type {
            match meter_type.as_str() {
                "residential" | "commercial" | "solar" | "industrial" => {}
                _ => return Err(ApiError::validation_error(
                    "Invalid meter_type filter. Allowed values: residential, commercial, solar, industrial",
                    Some("meter_type"),
                )),
            }
        }
        
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract client IP address from headers and connection info
fn extract_client_ip(headers: &HeaderMap, remote_addr: Option<std::net::SocketAddr>) -> Option<IpAddr> {
    // Try X-Forwarded-For header first (reverse proxy)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // X-Forwarded-For can contain multiple IPs, take the first one
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse() {
                    return Some(ip);
                }
            }
        }
    }
    
    // Try X-Real-IP header
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(real_ip_str) = real_ip.to_str() {
            if let Ok(ip) = real_ip_str.parse() {
                return Some(ip);
            }
        }
    }
    
    // Fall back to remote address
    if let Some(addr) = remote_addr {
        Some(addr.ip())
    } else {
        None
    }
}

/// Extract User-Agent header
fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

// ============================================================================
// Handler Functions
// ============================================================================

/// Verify meter ownership
/// POST /api/meters/verify
/// 
/// Prosumers verify ownership of their smart meters to submit readings
#[utoipa::path(
    post,
    path = "/api/meters/verify",
    tag = "meter-verification",
    request_body = VerifyMeterRequestWrapper,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Meter verification successful", body = VerifyMeterResponse),
        (status = 400, description = "Invalid meter data or meter already claimed"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Only prosumers can verify meters"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn verify_meter_handler(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    headers: HeaderMap,
    // Note: We would need to use ConnectInfo to get the actual remote address
    // For now, we'll use headers only
    Json(request_wrapper): Json<VerifyMeterRequestWrapper>,
) -> Result<Json<VerifyMeterResponse>, ApiError> {
    let request = request_wrapper.request;
    
    info!(
        "User {} attempting to verify meter: {}",
        user.sub, request.meter_serial
    );

    // Verify user is a prosumer
    if user.role != "prosumer" && user.role != "admin" {
        return Err(ApiError::Forbidden(
            "Only prosumers can verify meters".to_string(),
        ));
    }

    // Extract client information for audit
    let ip_address = extract_client_ip(&headers, None)
        .map(|ip| IpNetwork::from(ip));
    let user_agent = extract_user_agent(&headers);

    // Validate verification method
    match request.verification_method.as_str() {
        "serial" | "api_key" | "qr_code" | "challenge" => {}
        _ => return Err(ApiError::validation_error(
            "Invalid verification method. Allowed values: serial, api_key, qr_code, challenge",
            Some("verification_method"),
        )),
    }

    // Validate meter type
    match request.meter_type.as_str() {
        "residential" | "commercial" | "solar" | "industrial" => {}
        _ => return Err(ApiError::validation_error(
            "Invalid meter type. Allowed values: residential, commercial, solar, industrial",
            Some("meter_type"),
        )),
    }

    // Call verification service
    let response = state.meter_verification_service
        .verify_meter(
            user.sub,
            request,
            ip_address,
            user_agent,
        )
        .await
        .map_err(|e| {
            error!("Meter verification failed: {}", e);
            if e.to_string().contains("already registered") || 
               e.to_string().contains("already verified") ||
               e.to_string().contains("Meter key must be") {
                ApiError::BadRequest(e.to_string())
            } else {
                ApiError::Internal(format!("Meter verification failed: {}", e))
            }
        })?;

    info!(
        "Meter {} verified successfully for user {}",
        response.meter_id, user.sub
    );

    Ok(Json(response))
}

/// Get user's registered meters
/// GET /api/meters/registered
/// 
/// Get list of meters registered by the current user
#[utoipa::path(
    get,
    path = "/api/meters/registered",
    tag = "meter-verification",
    params(GetMetersQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's registered meters", body = GetMetersResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_registered_meters_handler(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    mut query: axum::extract::Query<GetMetersQuery>,
) -> Result<Json<GetMetersResponse>, ApiError> {
    info!("User {} fetching their registered meters", user.sub);

    // Validate query parameters
    query.validate()?;

    // Get all user meters
    let all_meters = state.meter_verification_service
        .get_user_meters(&user.sub)
        .await
        .map_err(|e| {
            error!("Failed to fetch user meters: {}", e);
            ApiError::Internal(format!("Failed to fetch meters: {}", e))
        })?;

    // Apply filters
    let filtered_meters: Vec<MeterRegistry> = all_meters
        .into_iter()
        .filter(|meter| {
            // Status filter
            if let Some(status_filter) = &query.status {
                if meter.verification_status != *status_filter {
                    return false;
                }
            }
            
            // Meter type filter
            if let Some(meter_type_filter) = &query.meter_type {
                if meter.meter_type.as_deref() != Some(meter_type_filter.as_str()) {
                    return false;
                }
            }
            
            true
        })
        .collect();

    // Apply pagination
    let total = filtered_meters.len() as i64;
    let start = ((query.page - 1) * query.page_size) as usize;
    let end = std::cmp::min(start + query.page_size as usize, filtered_meters.len());
    
    let paginated_meters = if start < filtered_meters.len() {
        filtered_meters[start..end].to_vec()
    } else {
        vec![]
    };

    // Convert to response format
    let meters_response: Vec<MeterRegistryResponse> = paginated_meters
        .into_iter()
        .map(MeterRegistryResponse::from)
        .collect();

    info!(
        "Returning {} meters for user {} (total: {})",
        meters_response.len(), user.sub, total
    );

    Ok(Json(GetMetersResponse {
        meters: meters_response,
        total,
    }))
}

/// Get verification statistics (admin only)
/// GET /api/admin/meters/verification-stats
/// 
/// Get system-wide meter verification statistics
#[utoipa::path(
    get,
    path = "/api/admin/meters/verification-stats",
    tag = "meter-verification",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Verification statistics", body = VerificationStats),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_verification_stats_handler(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<VerificationStats>, ApiError> {
    // Check admin permission
    if user.role != "admin" {
        return Err(ApiError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    info!("Admin {} fetching verification statistics", user.sub);

    let stats = state.meter_verification_service
        .get_verification_stats()
        .await
        .map_err(|e| {
            error!("Failed to fetch verification stats: {}", e);
            ApiError::Internal(format!("Failed to fetch statistics: {}", e))
        })?;

    Ok(Json(stats))
}

/// Health check for meter verification service
/// GET /api/meters/verification-health
/// 
/// Check if meter verification service is operational
#[utoipa::path(
    get,
    path = "/api/meters/verification-health",
    tag = "meter-verification",
    responses(
        (status = 200, description = "Meter verification service is healthy"),
        (status = 503, description = "Meter verification service is unhealthy")
    )
)]
pub async fn verification_health_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Simple health check - try to get verification stats
    match state.meter_verification_service.get_verification_stats().await {
        Ok(_) => Ok(Json(serde_json::json!({
            "status": "healthy",
            "service": "meter_verification",
            "timestamp": chrono::Utc::now()
        }))),
        Err(e) => {
            error!("Meter verification service health check failed: {}", e);
            Err(ApiError::ExternalService(
                "Meter verification service is unavailable".to_string()
            ))
        }
    }
}
