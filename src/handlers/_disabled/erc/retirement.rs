use axum::{
    extract::{Path, State},
    Json,
};
use tracing::{error, info};

use super::types::ErcCertificateResponse;
use crate::{
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    AppState,
};

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
