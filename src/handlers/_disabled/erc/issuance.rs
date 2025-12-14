use axum::{extract::State, Json};
use rust_decimal::prelude::ToPrimitive;
use tracing::{error, info};
use uuid::Uuid;

use super::types::{ErcCertificateResponse, IssueErcRequest};
use crate::{
    auth::middleware::AuthenticatedUser,
    error::ApiError,
    error::Result, // Explicit import for clarity
    AppState,
};

/// Check if user has REC authority role
fn require_rec_authority(user: &crate::auth::Claims) -> Result<()> {
    if user.role != "rec" && user.role != "admin" {
        return Err(ApiError::Forbidden(
            "REC authority role required".to_string(),
        ));
    }
    Ok(())
}

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
    // Note: Assuming crate::services::erc::IssueErcRequest matches or we reconstruct it
    let cert_request = crate::services::erc::IssueErcRequest {
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
    let user_wallet =
        crate::services::blockchain::BlockchainService::parse_pubkey(&certificate.wallet_address)
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
    let governance_program_id = state
        .blockchain_service
        .governance_program_id()
        .map_err(|e| {
            error!("Failed to get governance program ID: {}", e);
            ApiError::with_code(
                crate::error::ErrorCode::InternalServerError,
                format!("Governance program not configured: {}", e),
            )
        })?;

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
        .update_certificate_tx_signature(certificate.id, &blockchain_signature.to_string())
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
