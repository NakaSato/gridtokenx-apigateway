use axum::{
    extract::{Path, Query, State},
    Json,
};
use tracing::{error, info};

use super::types::{
    CertificateStatsResponse, CertificatesResponse, ErcCertificateResponse, GetCertificatesQuery,
};
use crate::{
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    utils::{PaginationMeta, PaginationParams},
    AppState,
};

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
