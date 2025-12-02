use axum::{
    Json,
    extract::{Path, Query, State},
};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{
    AppState,
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    utils::{PaginationMeta, PaginationParams, SortOrder},
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct IssueErcRequest {
    pub wallet_address: String,
    pub meter_id: Option<String>,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub expiry_date: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErcCertificateResponse {
    pub id: Uuid,
    pub certificate_id: String,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    #[schema(value_type = String)]
    pub kwh_amount: Option<Decimal>,
    pub issue_date: Option<chrono::DateTime<chrono::Utc>>,
    pub expiry_date: Option<chrono::DateTime<chrono::Utc>>,
    pub issuer_wallet: Option<String>,
    pub status: String,
    pub blockchain_tx_signature: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
pub struct GetCertificatesQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
    pub status: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
fn default_sort_order() -> SortOrder {
    SortOrder::Desc
}
#[allow(dead_code)]
fn default_limit() -> i64 {
    50
}

impl GetCertificatesQuery {
    pub fn validate_params(&mut self) -> Result<()> {
        if self.page < 1 {
            return Err(ApiError::validation_error(
                "page must be >= 1",
                Some("page"),
            ));
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err(ApiError::validation_error(
                "page_size must be between 1 and 100",
                Some("page_size"),
            ));
        }

        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "issue_date" | "expiry_date" | "kwh_amount" | "status" => {}
                _ => {
                    return Err(ApiError::validation_error(
                        "sort_by must be one of: issue_date, expiry_date, kwh_amount, status",
                        Some("sort_by"),
                    ));
                }
            }
        }

        // Validate status if provided
        if let Some(status) = &self.status {
            use crate::utils::validation::Validator;
            Validator::validate_certificate_status(status)?;
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.page_size as i64
    }
    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("issue_date")
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificatesResponse {
    pub data: Vec<ErcCertificateResponse>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificateStatsResponse {
    pub total_certificates: i64,
    #[schema(value_type = String)]
    pub active_kwh: Decimal,
    #[schema(value_type = String)]
    pub retired_kwh: Decimal,
    #[schema(value_type = String)]
    pub total_kwh: Decimal,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if user has REC authority role
fn require_rec_authority(user: &crate::auth::Claims) -> Result<()> {
    if user.role != "rec" && user.role != "admin" {
        return Err(ApiError::Forbidden(
            "REC authority role required".to_string(),
        ));
    }
    Ok(())
}

// ============================================================================
// Handler Functions
// ============================================================================

/// Issue a new ERC certificate
#[utoipa::path(
    post,
    path = "/api/erc/issue",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    request_body = IssueErcRequest,
    responses(
        (status = 200, description = "Certificate issued successfully", body = ErcCertificateResponse),
        (status = 400, description = "Invalid request data"),
        (status = 403, description = "Requires REC authority role"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn issue_certificate(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<IssueErcRequest>,
) -> Result<Json<ErcCertificateResponse>> {
    // Check REC authority permission
    require_rec_authority(&user)?;

    info!(
        "REC authority {} issuing certificate for {} kWh",
        user.sub, request.kwh_amount
    );

    // Get issuer wallet from database
    let issuer_record = sqlx::query!("SELECT wallet_address FROM users WHERE id = $1", user.sub)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to fetch issuer: {}", e);
            ApiError::Internal("Failed to fetch issuer data".to_string())
        })?;

    let issuer_wallet = issuer_record
        .wallet_address
        .ok_or_else(|| ApiError::BadRequest("Issuer wallet address not set".to_string()))?;

    // Get recipient user ID
    let recipient = sqlx::query!(
        "SELECT id FROM users WHERE wallet_address = $1",
        request.wallet_address
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch recipient: {}", e);
        ApiError::Internal("Failed to fetch recipient".to_string())
    })?;

    let recipient_user_id = recipient.map(|r| r.id).unwrap_or_else(|| Uuid::nil()); // Use nil UUID if user not found

    // Issue certificate in database first
    let cert_request = crate::services::erc_service::IssueErcRequest {
        wallet_address: request.wallet_address.clone(),
        meter_id: request.meter_id.clone(),
        kwh_amount: request.kwh_amount.clone(),
        expiry_date: request.expiry_date,
        metadata: request.metadata.clone(),
    };

    let certificate = state
        .erc_service
        .issue_certificate(recipient_user_id, &issuer_wallet, cert_request)
        .await
        .map_err(|e| {
            error!("Failed to issue certificate: {}", e);
            ApiError::Internal(format!("Failed to issue certificate: {}", e))
        })?;

    // Parse user wallet for blockchain operation
    let user_wallet = crate::services::blockchain_service::BlockchainService::parse_pubkey(
        &certificate.wallet_address,
    )
    .map_err(|e| {
        error!("Failed to parse user wallet: {}", e);
        ApiError::BadRequest(format!("Invalid wallet address: {}", e))
    })?;

    // Get authority keypair for blockchain operation
    let authority = state
        .wallet_service
        .get_authority_keypair()
        .await
        .map_err(|e| {
            error!("Failed to get authority keypair: {}", e);
            ApiError::with_code(
                crate::error::ErrorCode::ServiceUnavailable,
                format!("Authority wallet unavailable: {}", e),
            )
        })?;

    // Get governance program ID
    let governance_program_id =
        crate::services::blockchain_service::BlockchainService::governance_program_id().map_err(
            |e| {
                error!("Failed to get governance program ID: {}", e);
                ApiError::with_code(
                    crate::error::ErrorCode::InternalServerError,
                    format!("Governance program not configured: {}", e),
                )
            },
        )?;

    // Extract renewable source and validation data from metadata
    let (renewable_source, validation_data) = if let Some(metadata) = &certificate.metadata {
        let renewable_source = metadata
            .get("renewable_source")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let validation_data = metadata
            .get("validation_data")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        (renewable_source, validation_data)
    } else {
        ("Unknown".to_string(), "".to_string())
    };

    // Convert kwh_amount to f64 for blockchain
    let kwh_amount_f64 = certificate
        .kwh_amount
        .as_ref()
        .and_then(|bd| bd.to_f64())
        .unwrap_or(0.0);

    // Get meter_id
    let meter_id = request.meter_id.clone().ok_or_else(|| {
        ApiError::BadRequest("meter_id is required for on-chain issuance".to_string())
    })?;

    // Mint certificate on-chain
    let blockchain_signature = state
        .erc_service
        .issue_certificate_on_chain(
            &certificate.certificate_id,
            &user_wallet,
            &meter_id,
            kwh_amount_f64,
            &renewable_source,
            &validation_data,
            &authority,
            &governance_program_id,
        )
        .await
        .map_err(|e| {
            error!("Failed to mint certificate on-chain: {}", e);
            ApiError::Internal(format!("Blockchain minting failed: {}", e))
        })?;

    // Update certificate with blockchain signature
    let updated_certificate = state
        .erc_service
        .update_blockchain_signature(
            &certificate.certificate_id,
            &blockchain_signature.to_string(),
        )
        .await
        .map_err(|e| {
            error!("Failed to update certificate blockchain signature: {}", e);
            ApiError::Internal(format!("Failed to update database: {}", e))
        })?;

    info!(
        "Certificate {} issued and minted on-chain: {}",
        certificate.certificate_id, blockchain_signature
    );

    Ok(Json(ErcCertificateResponse {
        id: updated_certificate.id,
        certificate_id: updated_certificate.certificate_id,
        user_id: updated_certificate.user_id,
        wallet_address: updated_certificate.wallet_address,
        kwh_amount: updated_certificate.kwh_amount,
        issue_date: updated_certificate.issue_date,
        expiry_date: updated_certificate.expiry_date,
        issuer_wallet: updated_certificate.issuer_wallet,
        status: updated_certificate.status,
        blockchain_tx_signature: Some(blockchain_signature.to_string()),
        metadata: updated_certificate.metadata,
    }))
}

/// Get certificate by ID
#[utoipa::path(
    get,
    path = "/api/erc/{certificate_id}",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    params(
        ("certificate_id" = String, Path, description = "Certificate ID")
    ),
    responses(
        (status = 200, description = "Certificate details", body = ErcCertificateResponse),
        (status = 404, description = "Certificate not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_certificate(
    State(state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
    Path(certificate_id): Path<String>,
) -> Result<Json<ErcCertificateResponse>> {
    info!("Fetching certificate: {}", certificate_id);

    let certificate = state
        .erc_service
        .get_certificate_by_id(&certificate_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch certificate: {}", e);
            ApiError::NotFound(format!("Certificate not found: {}", e))
        })?;

    Ok(Json(ErcCertificateResponse {
        id: certificate.id,
        certificate_id: certificate.certificate_id,
        user_id: certificate.user_id,
        wallet_address: certificate.wallet_address,
        kwh_amount: certificate.kwh_amount,
        issue_date: certificate.issue_date,
        expiry_date: certificate.expiry_date,
        issuer_wallet: certificate.issuer_wallet,
        status: certificate.status,
        blockchain_tx_signature: certificate.blockchain_tx_signature,
        metadata: certificate.metadata,
    }))
}

/// Get certificates by wallet address
#[utoipa::path(
    get,
    path = "/api/erc/wallet/{wallet_address}",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    params(
        ("wallet_address" = String, Path, description = "Wallet address"),
        ("limit" = Option<i64>, Query, description = "Maximum certificates to return (default: 50)"),
        ("offset" = Option<i64>, Query, description = "Number of certificates to skip (default: 0)")
    ),
    responses(
        (status = 200, description = "List of certificates", body = Vec<ErcCertificateResponse>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_certificates_by_wallet(
    State(state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
    Path(wallet_address): Path<String>,
    Query(query): Query<GetCertificatesQuery>,
) -> Result<Json<Vec<ErcCertificateResponse>>> {
    info!("Fetching certificates for wallet: {}", wallet_address);

    let certificates = state
        .erc_service
        .get_certificates_by_wallet(&wallet_address, query.limit(), query.offset())
        .await
        .map_err(|e| {
            error!("Failed to fetch certificates: {}", e);
            ApiError::Internal(format!("Failed to fetch certificates: {}", e))
        })?;

    let response: Vec<ErcCertificateResponse> = certificates
        .into_iter()
        .map(|cert| ErcCertificateResponse {
            id: cert.id,
            certificate_id: cert.certificate_id,
            user_id: cert.user_id,
            wallet_address: cert.wallet_address,
            kwh_amount: cert.kwh_amount,
            issue_date: cert.issue_date,
            expiry_date: cert.expiry_date,
            issuer_wallet: cert.issuer_wallet,
            status: cert.status,
            blockchain_tx_signature: cert.blockchain_tx_signature,
            metadata: cert.metadata,
        })
        .collect();

    Ok(Json(response))
}

/// Get current user's certificates with pagination
#[utoipa::path(
    get,
    path = "/api/erc/my-certificates",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    params(GetCertificatesQuery),
    responses(
        (status = 200, description = "User's certificates", body = CertificatesResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_my_certificates(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(mut query): Query<GetCertificatesQuery>,
) -> Result<Json<CertificatesResponse>> {
    info!("User {} fetching their certificates", user.sub);

    // Validate parameters
    query.validate_params()?;

    let limit = query.limit();
    let offset = query.offset();
    let sort_field = query.get_sort_field();
    let sort_direction = query.sort_direction();
    let status_filter = query.status.as_deref();

    // Count total
    let total = state
        .erc_service
        .count_user_certificates(user.sub, status_filter)
        .await
        .map_err(|e| {
            error!("Failed to count user certificates: {}", e);
            ApiError::Internal(format!("Failed to count certificates: {}", e))
        })?;

    // Fetch certificates
    let certificates = state
        .erc_service
        .get_user_certificates(
            user.sub,
            limit,
            offset,
            sort_field,
            sort_direction,
            status_filter,
        )
        .await
        .map_err(|e| {
            error!("Failed to fetch user certificates: {}", e);
            ApiError::Internal(format!("Failed to fetch certificates: {}", e))
        })?;

    let data: Vec<ErcCertificateResponse> = certificates
        .into_iter()
        .map(|cert| ErcCertificateResponse {
            id: cert.id,
            certificate_id: cert.certificate_id,
            user_id: cert.user_id,
            wallet_address: cert.wallet_address,
            kwh_amount: cert.kwh_amount,
            issue_date: cert.issue_date,
            expiry_date: cert.expiry_date,
            issuer_wallet: cert.issuer_wallet,
            status: cert.status,
            blockchain_tx_signature: cert.blockchain_tx_signature,
            metadata: cert.metadata,
        })
        .collect();

    // Create pagination metadata
    let pagination = PaginationMeta::new(
        &PaginationParams {
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_order: query.sort_order,
        },
        total,
    );

    Ok(Json(CertificatesResponse { data, pagination }))
}

/// Get certificate statistics for current user
#[utoipa::path(
    get,
    path = "/api/erc/my-stats",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Certificate statistics", body = CertificateStatsResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_my_certificate_stats(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<CertificateStatsResponse>> {
    info!("User {} fetching certificate statistics", user.sub);

    let stats = state
        .erc_service
        .get_user_stats(user.sub)
        .await
        .map_err(|e| {
            error!("Failed to fetch certificate stats: {}", e);
            ApiError::Internal("Failed to fetch statistics".to_string())
        })?;

    Ok(Json(CertificateStatsResponse {
        total_certificates: stats.total_certificates,
        active_kwh: stats.active_kwh,
        retired_kwh: stats.retired_kwh,
        total_kwh: stats.total_kwh,
    }))
}

/// Retire a certificate (admin/owner only)
#[utoipa::path(
    post,
    path = "/api/erc/{certificate_id}/retire",
    tag = "erc",
    security(
        ("bearer_auth" = [])
    ),
    params(
        ("certificate_id" = String, Path, description = "Certificate ID to retire")
    ),
    responses(
        (status = 200, description = "Certificate retired successfully", body = ErcCertificateResponse),
        (status = 403, description = "Not authorized to retire this certificate"),
        (status = 404, description = "Certificate not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn retire_certificate(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(certificate_id): Path<String>,
) -> Result<Json<ErcCertificateResponse>> {
    info!("User {} retiring certificate {}", user.sub, certificate_id);

    // Get certificate to verify ownership
    let certificate = state
        .erc_service
        .get_certificate_by_id(&certificate_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch certificate: {}", e);
            ApiError::NotFound(format!("Certificate not found: {}", e))
        })?;

    // Check if user owns the certificate or is admin
    if certificate.user_id != Some(user.sub) && user.role != "admin" {
        return Err(ApiError::Forbidden(
            "You can only retire your own certificates".to_string(),
        ));
    }

    // Retire certificate
    let retired_cert = state
        .erc_service
        .retire_certificate(certificate.id)
        .await
        .map_err(|e| {
            error!("Failed to retire certificate: {}", e);
            ApiError::Internal(format!("Failed to retire certificate: {}", e))
        })?;

    info!("Certificate {} retired successfully", certificate_id);

    Ok(Json(ErcCertificateResponse {
        id: retired_cert.id,
        certificate_id: retired_cert.certificate_id,
        user_id: retired_cert.user_id,
        wallet_address: retired_cert.wallet_address,
        kwh_amount: retired_cert.kwh_amount,
        issue_date: retired_cert.issue_date,
        expiry_date: retired_cert.expiry_date,
        issuer_wallet: retired_cert.issuer_wallet,
        status: retired_cert.status,
        blockchain_tx_signature: retired_cert.blockchain_tx_signature,
        metadata: retired_cert.metadata,
    }))
}
