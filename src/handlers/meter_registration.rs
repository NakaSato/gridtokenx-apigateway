use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tracing::{error, info};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::{
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    AppState,
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMeterRequest {
    pub meter_serial: String,
    pub meter_public_key: String, // Base58 encoded Ed25519 public key
    pub meter_type: String,       // residential, commercial, solar, industrial
    pub location_address: Option<String>,
    pub manufacturer: Option<String>,
    pub installation_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterMeterResponse {
    pub meter_id: Uuid,
    pub meter_serial: String,
    pub wallet_address: String,
    pub verification_status: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterInfo {
    pub id: Uuid,
    pub meter_serial: String,
    pub meter_type: Option<String>,
    pub location_address: Option<String>,
    pub verification_status: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyMeterRequest {
    pub verification_status: String, // "verified" or "rejected"
    pub verification_proof: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyMeterResponse {
    pub meter_id: Uuid,
    pub verification_status: String,
    pub verified_at: chrono::DateTime<chrono::Utc>,
    pub message: String,
}

// ============================================================================
// Handler Functions
// ============================================================================

/// Register a new smart meter
/// POST /api/user/meters
#[utoipa::path(
    post,
    path = "/api/user/meters",
    tag = "meters",
    request_body = RegisterMeterRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 201, description = "Meter registered successfully", body = RegisterMeterResponse),
        (status = 400, description = "Invalid data or meter already registered"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Email not verified or wallet not assigned"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn register_meter(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<RegisterMeterRequest>,
) -> Result<Json<RegisterMeterResponse>> {
    info!(
        "User {} registering meter: {}",
        user.sub, request.meter_serial
    );

    // Validate user has email verified and wallet address
    let user_record = sqlx::query!(
        "SELECT email_verified, wallet_address FROM users WHERE id = $1",
        user.sub
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Internal("Failed to fetch user data".to_string())
    })?;

    if !user_record.email_verified {
        return Err(ApiError::Forbidden(
            "Email must be verified before registering meters".to_string(),
        ));
    }

    let wallet_address = user_record.wallet_address.ok_or_else(|| {
        ApiError::Forbidden("Wallet address not assigned. Please contact support.".to_string())
    })?;

    // Validate meter serial format
    if request.meter_serial.is_empty() || request.meter_serial.len() > 255 {
        return Err(ApiError::BadRequest(
            "Invalid meter serial number".to_string(),
        ));
    }

    // Validate public key format (base58, should decode to 32 bytes)
    let public_key_bytes = bs58::decode(&request.meter_public_key)
        .into_vec()
        .map_err(|e| {
            error!("Invalid public key base58: {}", e);
            ApiError::BadRequest(format!("Invalid public key format: {}", e))
        })?;

    if public_key_bytes.len() != 32 {
        return Err(ApiError::BadRequest(format!(
            "Invalid public key length: expected 32 bytes, got {}",
            public_key_bytes.len()
        )));
    }

    // Check if meter already registered
    let existing = sqlx::query!(
        "SELECT id FROM meter_registry WHERE meter_serial = $1",
        request.meter_serial
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to check existing meter: {}", e);
        ApiError::Internal("Database error".to_string())
    })?;

    if existing.is_some() {
        return Err(ApiError::BadRequest(
            "Meter serial number already registered".to_string(),
        ));
    }

    // Hash the public key for storage
    let meter_key_hash = format!("{:x}", sha2::Sha256::digest(&public_key_bytes));

    // Insert meter registration
    let meter_id = sqlx::query_scalar!(
        r#"
        INSERT INTO meter_registry (
            user_id,
            meter_serial,
            meter_key_hash,
            meter_public_key,
            verification_status,
            meter_type,
            location_address,
            manufacturer,
            installation_date,
            created_at,
            updated_at
        ) VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, NOW(), NOW())
        RETURNING id
        "#,
        user.sub,
        request.meter_serial,
        meter_key_hash,
        request.meter_public_key,
        request.meter_type,
        request.location_address,
        request.manufacturer,
        request.installation_date
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to insert meter registration: {}", e);
        ApiError::Internal("Failed to register meter".to_string())
    })?;

    // INTEGRATION: Register meter on-chain (as Gateway Authority)
    // We use the Gateway's authority keypair to register the meter on-chain.
    // This effectively makes the Gateway the on-chain "owner" (custodian) of the meter record.
    if let Ok(authority_keypair) = state.blockchain_service.get_authority_keypair().await {
        let meter_type_u8: u8 = match request.meter_type.to_lowercase().as_str() {
            "solar" => 0,
            "wind" => 1,
            "battery" => 2,
            _ => 3, // Grid/Other
        };

        match state
            .blockchain_service
            .register_meter_on_chain(&authority_keypair, &request.meter_serial, meter_type_u8)
            .await
        {
            Ok(sig) => {
                tracing::info!("Meter registered on-chain successfully. Signature: {}", sig);
            }
            Err(e) => {
                tracing::error!("Failed to register meter on-chain: {}", e);
                // Non-blocking error
            }
        }
    } else {
        tracing::error!("Failed to load authority keypair for on-chain registration");
    }

    info!(
        "Meter registered successfully: {} (ID: {})",
        request.meter_serial, meter_id
    );

    Ok(Json(RegisterMeterResponse {
        meter_id,
        meter_serial: request.meter_serial,
        wallet_address,
        verification_status: "pending".to_string(),
        message: "Meter registered successfully. Status is pending until verified by admin."
            .to_string(),
    }))
}

/// Get user's registered meters
/// GET /api/user/meters
#[utoipa::path(
    get,
    path = "/api/user/meters",
    tag = "meters",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's registered meters", body = Vec<MeterInfo>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_meters(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<Vec<MeterInfo>>> {
    info!("User {} fetching their registered meters", user.sub);

    let meters = sqlx::query_as!(
        MeterInfo,
        r#"
        SELECT 
            id,
            meter_serial,
            meter_type,
            location_address,
            verification_status,
            verified_at,
            created_at
        FROM meter_registry
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
        user.sub
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user meters: {}", e);
        ApiError::Internal("Failed to fetch meters".to_string())
    })?;

    Ok(Json(meters))
}

/// Verify a meter (admin only)
/// POST /api/admin/meters/:id/verify
#[utoipa::path(
    post,
    path = "/api/admin/meters/{id}/verify",
    tag = "meters",
    request_body = VerifyMeterRequest,
    security(("bearer_auth" = [])),
    params(
        ("id" = Uuid, Path, description = "Meter ID")
    ),
    responses(
        (status = 200, description = "Meter verification updated", body = VerifyMeterResponse),
        (status = 400, description = "Invalid verification status"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "Meter not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn verify_meter(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(meter_id): Path<Uuid>,
    Json(request): Json<VerifyMeterRequest>,
) -> Result<Json<VerifyMeterResponse>> {
    info!(
        "Admin {} verifying meter {} with status: {}",
        user.sub, meter_id, request.verification_status
    );

    // Check if user is admin
    let admin_user = sqlx::query!(
        "SELECT role::text as role FROM users WHERE id = $1",
        user.sub
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch admin user: {}", e);
        ApiError::Database(e)
    })?;

    if admin_user.role.as_deref() != Some("admin")
        && admin_user.role.as_deref() != Some("super_admin")
    {
        return Err(ApiError::Forbidden(
            "Only admins can verify meters".to_string(),
        ));
    }

    // Validate verification status
    if request.verification_status != "verified" && request.verification_status != "rejected" {
        return Err(ApiError::BadRequest(
            "Verification status must be 'verified' or 'rejected'".to_string(),
        ));
    }

    // Check if meter exists
    let meter = sqlx::query!(
        "SELECT id, meter_serial FROM meter_registry WHERE id = $1",
        meter_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch meter: {}", e);
        ApiError::Internal("Database error".to_string())
    })?
    .ok_or_else(|| ApiError::NotFound("Meter not found".to_string()))?;

    // Update verification status
    let verified_at = chrono::Utc::now();

    sqlx::query!(
        r#"
        UPDATE meter_registry
        SET 
            verification_status = $1,
            verified_at = $2,
            verified_by = $3,
            verification_proof = $4,
            updated_at = NOW()
        WHERE id = $5
        "#,
        request.verification_status,
        verified_at,
        user.sub,
        request.verification_proof,
        meter_id
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to update meter verification: {}", e);
        ApiError::Internal("Failed to update verification status".to_string())
    })?;

    info!(
        "Meter {} verification updated to: {}",
        meter.meter_serial, request.verification_status
    );

    Ok(Json(VerifyMeterResponse {
        meter_id,
        verification_status: request.verification_status.clone(),
        verified_at,
        message: format!(
            "Meter {} successfully",
            if request.verification_status == "verified" {
                "verified"
            } else {
                "rejected"
            }
        ),
    }))
}
