//! Meter management handlers.
//!
//! This module provides endpoints for:
//! - Submitting meter readings
//! - Retrieving reading history
//! - Meter statistics and minting

use axum::{
    extract::{Path, Query, State},
    Json,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthenticatedUser,
    error::{ApiError, Result},
    handlers::{require_role, SortOrder},
    services::BlockchainService,
    utils::PaginationParams,
    AppState,
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitReadingRequest {
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub reading_timestamp: chrono::DateTime<chrono::Utc>,
    pub meter_signature: Option<String>,
    /// NEW: Required UUID from meter_registry (for verified meters)
    /// For legacy support, this can be omitted during grace period
    pub meter_id: Option<Uuid>,
    /// Legacy meter serial number (for unverified meters)
    pub meter_serial: Option<String>,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub wallet_address: String,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub reading_timestamp: chrono::DateTime<chrono::Utc>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
}

/// Query parameters for meter readings
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct GetReadingsQuery {
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field: "submitted_at", "reading_timestamp", "kwh_amount"
    #[serde(default = "default_sort_field")]
    pub sort_by: String,

    /// Sort direction: "asc" or "desc"
    #[serde(default)]
    pub sort_order: SortOrder,

    /// Filter by minted status
    pub minted: Option<bool>,
}

fn default_page() -> u32 {
    crate::constants::pagination::DEFAULT_PAGE
}

fn default_per_page() -> u32 {
    crate::constants::pagination::DEFAULT_PER_PAGE
}

fn default_sort_field() -> String {
    "submitted_at".to_string()
}

impl GetReadingsQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] =
        &["submitted_at", "reading_timestamp", "kwh_amount"];

    pub fn validate(&mut self) -> Result<()> {
        // Normalize pagination
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self
            .per_page
            .clamp(1, crate::constants::pagination::MAX_PER_PAGE);

        // Validate sort field
        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }

    pub fn get_sort_field(&self) -> &str {
        &self.sort_by
    }

    /// Alias for per_page for backward compatibility
    pub fn page_size(&self) -> u32 {
        self.per_page
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterReadingsResponse {
    pub data: Vec<MeterReadingResponse>,
    pub pagination: crate::utils::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MintFromReadingRequest {
    pub reading_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MintResponse {
    pub message: String,
    pub transaction_signature: String,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub wallet_address: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserStatsResponse {
    pub total_readings: i64,
    #[schema(value_type = String)]
    pub unminted_kwh: Decimal,
    #[schema(value_type = String)]
    pub minted_kwh: Decimal,
    #[schema(value_type = String)]
    pub total_kwh: Decimal,
}

// ============================================================================
// Handler Functions
// ============================================================================

/// Submit a new meter reading
/// POST /api/meters/submit-reading
///
/// Prosumers submit their smart meter readings for token minting
#[utoipa::path(
    post,
    path = "/api/meters/submit-reading",
    tag = "meters",
    request_body = SubmitReadingRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Meter reading submitted successfully", body = MeterReadingResponse),
        (status = 400, description = "Invalid reading data or wallet not set"),
        (status = 403, description = "Forbidden - Only prosumers can submit readings"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<SubmitReadingRequest>,
) -> Result<Json<MeterReadingResponse>> {
    info!(
        "User {} submitting meter reading: {} kWh",
        user.sub, request.kwh_amount
    );

    // Verify user is a prosumer or admin or AMI (simulator)
    if user.role != "prosumer" && user.role != "admin" && user.role != "ami" {
        return Err(ApiError::Forbidden(
            "Only prosumers or AMI can submit meter readings".to_string(),
        ));
    }

    let wallet_address = if user.role == "ami" {
        // For AMI/Simulator, get wallet address from request
        request.wallet_address.clone().ok_or_else(|| {
            ApiError::BadRequest("Wallet address required for AMI submission".to_string())
        })?
    } else {
        // Validate wallet address - get from user object in database
        let user_record = sqlx::query!("SELECT wallet_address FROM users WHERE id = $1", user.sub)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!("Failed to fetch user: {}", e);
                ApiError::Internal("Failed to fetch user data".to_string())
            })?;

        user_record
            .wallet_address
            .ok_or_else(|| ApiError::BadRequest("Wallet address not set for user".to_string()))?
    };

    // Verify meter signature if provided
    info!(
        "Received reading submission. Serial: {:?}, Signature present: {}",
        request.meter_serial,
        request.meter_signature.is_some()
    );

    // Track resolved meter ID from registry lookup
    let mut resolved_meter_id = request.meter_id;

    if let (Some(meter_serial), Some(signature)) = (&request.meter_serial, &request.meter_signature)
    {
        info!("Verifying signature for meter: {}", meter_serial);

        // Lookup meter registration and public key
        let meter_record = sqlx::query!(
            "SELECT id, meter_public_key, user_id, verification_status FROM meter_registry WHERE meter_serial = $1",
            meter_serial
        )
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to lookup meter registration: {}", e);
            ApiError::Internal("Failed to verify meter".to_string())
        })?;

        if let Some(meter) = meter_record {
            // Check if meter is verified
            if meter.verification_status.as_str() != "verified" {
                return Err(ApiError::Forbidden(
                    "Meter is not verified. Please wait for admin approval.".to_string(),
                ));
            }

            // Verify wallet ownership
            let meter_owner_id = meter.user_id;

            // Get meter owner's wallet address
            let owner_record = sqlx::query!(
                "SELECT wallet_address FROM users WHERE id = $1",
                meter_owner_id
            )
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!("Failed to fetch meter owner: {}", e);
                ApiError::Internal("Failed to verify meter ownership".to_string())
            })?;

            let owner_wallet = owner_record
                .wallet_address
                .ok_or_else(|| {
                    // This happens if the user registered but hasn't logged in yet (new flow)
                    ApiError::BadRequest("Meter owner has not initialized their wallet. Please login to the dashboard first.".to_string())
                })?;

            // Verify wallet address matches
            if owner_wallet != wallet_address {
                return Err(ApiError::Forbidden(
                    "Wallet address does not match meter owner".to_string(),
                ));
            }

            // Verify signature if public key is available
            if let Some(public_key) = &meter.meter_public_key {
                let public_key = public_key.as_str();
                info!("Verifying signature with public key");

                // Create canonical message
                let message = crate::utils::MeterReadingMessage::new(
                    meter_serial.clone(),
                    request.reading_timestamp,
                    request.kwh_amount,
                    wallet_address.clone(),
                );

                // Auto-resolve meter_id if not provided but found in registry
                if resolved_meter_id.is_none() {
                    resolved_meter_id = Some(meter.id);
                    info!("Auto-resolved verified meter_id from serial: {}", meter.id);
                }

                // Verify signature
                match crate::utils::verify_signature(&public_key, signature, &message) {
                    Ok(true) => {
                        info!(
                            "Signature verification successful for meter: {}",
                            meter_serial
                        );
                    }
                    Ok(false) => {
                        error!("Invalid signature for meter: {}", meter_serial);
                        return Err(ApiError::Unauthorized(
                            "Invalid meter signature".to_string(),
                        ));
                    }
                    Err(e) => {
                        error!("Signature verification error: {}", e);
                        return Err(ApiError::BadRequest(format!(
                            "Signature verification failed: {}",
                            e
                        )));
                    }
                }
            } else {
                warn!(
                    "Meter {} has no public key registered - skipping signature verification",
                    meter_serial
                );
            }
        } else {
            warn!(
                "Meter {} not found in registry - allowing legacy submission",
                meter_serial
            );
        }
    }

    // Verify meter ownership if meter_id is provided
    let (verification_status, meter_owner_id) = if let Some(meter_id) = resolved_meter_id {
        // Verify meter ownership
        let is_owner = if user.role == "ami" {
            true // AMI is trusted
        } else {
            state
                .meter_verification_service
                .verify_meter_ownership(&user.sub.to_string(), &meter_id)
                .await
                .map_err(|e| {
                    error!("Failed to verify meter ownership: {}", e);
                    ApiError::Internal(format!("Failed to verify meter ownership: {}", e))
                })?
        };

        if !is_owner {
            return Err(ApiError::Forbidden(
                "You do not own this meter or it is not verified".to_string(),
            ));
        }

        info!("Meter ownership verified for meter_id: {}", meter_id);

        // Lookup owner for AMI
        let owner = if user.role == "ami" {
            let m = sqlx::query!("SELECT user_id FROM meter_registry WHERE id = $1", meter_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or(ApiError::BadRequest("Meter not found".to_string()))?;
            Some(m.user_id)
        } else {
            Some(user.sub)
        };

        ("verified", owner)
    } else {
        // Legacy support during grace period
        warn!(
            "User {} submitting reading without meter_id - using legacy_unverified status",
            user.sub
        );
        let owner = if user.role == "ami" {
            // For legacy AMI without meter_id, we need looking up by serial if possible or fail
            if let Some(serial) = &request.meter_serial {
                let m = sqlx::query!(
                    "SELECT user_id FROM meter_registry WHERE meter_serial = $1",
                    serial
                )
                .fetch_optional(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .ok_or(ApiError::BadRequest("Meter not found".to_string()))?;
                Some(m.user_id)
            } else {
                return Err(ApiError::BadRequest(
                    "AMI must provide meter_id or serial".to_string(),
                ));
            }
        } else {
            Some(user.sub)
        };
        ("legacy_unverified", owner)
    };

    let submission_user_id =
        meter_owner_id.ok_or(ApiError::Internal("No owner for meter".to_string()))?;

    // Broadcast meter reading received event via WebSocket
    let meter_serial = "default"; // Using default value since meter_serial is not available

    // Convert BigDecimal to f64 for WebSocket broadcast
    let kwh_amount_f64 = request
        .kwh_amount
        .to_f64()
        .ok_or_else(|| ApiError::Internal("Failed to convert kwh_amount to f64".to_string()))?;

    // Validate reading data
    let meter_request = crate::services::meter_service::SubmitMeterReadingRequest {
        wallet_address: wallet_address.clone(),
        kwh_amount: request.kwh_amount,
        reading_timestamp: request.reading_timestamp,
        meter_signature: request.meter_signature,
        meter_serial: request.meter_serial,
    };

    crate::services::meter_service::MeterService::validate_reading(&meter_request)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Submit reading to database with verification status
    let reading = state
        .meter_service
        .submit_reading_with_verification(
            submission_user_id,
            meter_request,
            resolved_meter_id,
            verification_status,
        )
        .await
        .map_err(|e| {
            error!("Failed to submit meter reading: {}", e);
            ApiError::Internal(format!("Failed to submit reading: {}", e))
        })?;

    let _ = state
        .websocket_service
        .broadcast_meter_reading_received(&user.sub, &wallet_address, meter_serial, kwh_amount_f64)
        .await;

    info!("Meter reading submitted successfully: {}", reading.id);

    // AUTO-MINT/BURN/TRANSFER LOGIC
    // Trigger blockchain transaction based on reading amount

    let kwh_val = request.kwh_amount;

    if !kwh_val.is_zero() {
        info!("Triggering blockchain action for reading {}", reading.id);

        // Get authority keypair
        let authority_keypair_result = state.wallet_service.get_authority_keypair().await;

        if let Ok(authority_keypair) = authority_keypair_result {
            // Get token mint
            let token_mint_result =
                BlockchainService::parse_pubkey(&state.config.energy_token_mint);
            let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

            if let (Ok(token_mint), Ok(wallet_pubkey)) = (token_mint_result, wallet_pubkey_result) {
                // Ensure user token account exists
                let user_token_account_result = state
                    .blockchain_service
                    .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                    .await;

                if let Ok(user_token_account) = user_token_account_result {
                    let amount_f64 = kwh_val.to_f64().unwrap_or(0.0);
                    let is_surplus = kwh_val > Decimal::ZERO;

                    if is_surplus {
                        // MINT
                        info!("Minting {} kWh for user {}", amount_f64, wallet_address);
                        let mint_sig = state
                            .blockchain_service
                            .mint_energy_tokens(
                                &authority_keypair,
                                &user_token_account,
                                &wallet_pubkey,
                                &token_mint,
                                amount_f64,
                            )
                            .await;

                        match mint_sig {
                            Ok(signature) => {
                                let sig_str = signature.to_string();
                                info!("Mint successful: {}", sig_str);
                                let _ = state
                                    .meter_service
                                    .mark_as_minted(reading.id, &sig_str)
                                    .await;

                                // AUTOMATIC P2P ROUTING (Corporate Buyer)
                                // Find a Corporate user (PEA/MEA) to transfer surplus to
                                use crate::database::schema::types::UserRole;
                                use uuid::Uuid;

                                let corporate_user = sqlx::query_as::<_, (Uuid, String)>(
                                    "SELECT id, wallet_address FROM users WHERE role = $1 AND wallet_address IS NOT NULL LIMIT 1"
                                )
                                .bind(UserRole::Corporate)
                                .fetch_optional(&state.db) // Use state.db for the database pool
                                .await;

                                match corporate_user {
                                    Ok(Some((_corp_id, corp_wallet_str))) => {
                                        info!(
                                            "Found Corporate User (PEA/MEA) with wallet: {}",
                                            corp_wallet_str
                                        );

                                        if let Ok(corp_wallet) =
                                            BlockchainService::parse_pubkey(&corp_wallet_str)
                                        {
                                            // Ensure Corporate token account exists
                                            if let Ok(corp_token_account) = state
                                                .blockchain_service
                                                .ensure_token_account_exists(
                                                    &authority_keypair,
                                                    &corp_wallet,
                                                    &token_mint,
                                                )
                                                .await
                                            {
                                                info!(
                                                    "Auto-Transferring {} kWh from User to Corporate User (PEA)",
                                                    amount_f64
                                                );

                                                let transfer_sig = state
                                                    .blockchain_service
                                                    .transfer_energy_tokens(
                                                        &authority_keypair, // acting as user signer for demo
                                                        &user_token_account,
                                                        &corp_token_account,
                                                        &token_mint,
                                                        amount_f64,
                                                    )
                                                    .await;

                                                match transfer_sig {
                                                    Ok(tsig) => info!(
                                                        "Transfer to Corporate User successful: {}",
                                                        tsig
                                                    ),
                                                    Err(e) => warn!(
                                                        "Transfer to Corporate User failed: {}",
                                                        e
                                                    ),
                                                }
                                            } else {
                                                warn!(
                                                    "Failed to ensure Corporate token account exists"
                                                );
                                            }
                                        } else {
                                            warn!("Invalid Corporate wallet address in database");
                                        }
                                    }
                                    Ok(None) => {
                                        info!(
                                            "No Corporate user found. User holds surplus tokens (P2P mode)."
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to query for Corporate user: {}", e);
                                    }
                                }
                            }
                            Err(e) => error!("Mint failed: {}", e),
                        }
                    } else {
                        // BURN
                        info!(
                            "Burning {} kWh from user {}",
                            amount_f64.abs(),
                            wallet_address
                        );
                        let burn_sig = state
                            .blockchain_service
                            .burn_energy_tokens(
                                &authority_keypair,
                                &user_token_account,
                                &token_mint,
                                amount_f64,
                            )
                            .await;

                        match burn_sig {
                            Ok(signature) => {
                                let sig_str = signature.to_string();
                                info!("Burn successful: {}", sig_str);
                                let _ = state
                                    .meter_service
                                    .mark_as_minted(reading.id, &sig_str)
                                    .await;
                            }
                            Err(e) => error!("Burn failed: {}", e),
                        }
                    }
                } else {
                    error!("Failed to ensure token account exists");
                }
            } else {
                error!("Invalid token mint or wallet address");
            }
        } else {
            warn!("Authority keypair not available - skipping blockchain action");
        }
    }

    // Fetch updated reading (if marked as minted)
    let final_reading = state
        .meter_service
        .get_reading_by_id(reading.id)
        .await
        .unwrap_or(reading);

    Ok(Json(MeterReadingResponse {
        id: final_reading.id,
        user_id: final_reading
            .user_id
            .ok_or_else(|| ApiError::Internal("Missing user_id".to_string()))?,
        wallet_address: final_reading.wallet_address,
        kwh_amount: final_reading
            .kwh_amount
            .ok_or_else(|| ApiError::Internal("Missing kwh_amount".to_string()))?,
        reading_timestamp: final_reading
            .reading_timestamp
            .ok_or_else(|| ApiError::Internal("Missing reading_timestamp".to_string()))?,
        submitted_at: final_reading
            .submitted_at
            .ok_or_else(|| ApiError::Internal("Missing submitted_at".to_string()))?,
        minted: final_reading.minted.unwrap_or(false),
        mint_tx_signature: final_reading.mint_tx_signature,
    }))
}

/// Get meter readings for current user
/// GET /api/meters/my-readings
#[utoipa::path(
    get,
    path = "/api/meters/my-readings",
    tag = "meters",
    params(GetReadingsQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's meter readings", body = Vec<MeterReadingResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_my_readings(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(mut query): Query<GetReadingsQuery>,
) -> Result<Json<MeterReadingsResponse>> {
    info!("User {} fetching their meter readings", user.sub);

    // Validate query parameters
    query.validate()?;

    let limit = query.limit();
    let offset = query.offset();
    let sort_by = query.get_sort_field();
    let sort_order = query.sort_direction();

    // Get total count
    let total = state
        .meter_service
        .count_user_readings(user.sub, query.minted)
        .await
        .map_err(|e| {
            error!("Failed to count user readings: {}", e);
            ApiError::Internal(format!("Failed to count readings: {}", e))
        })?;

    // Get readings with pagination
    let readings = state
        .meter_service
        .get_user_readings(user.sub, limit, offset, sort_by, sort_order, query.minted)
        .await
        .map_err(|e| {
            error!("Failed to fetch user readings: {}", e);
            ApiError::Internal(format!("Failed to fetch readings: {}", e))
        })?;

    let data: Vec<MeterReadingResponse> = readings
        .into_iter()
        .filter_map(|r| {
            // Only include readings with all required fields
            Some(MeterReadingResponse {
                id: r.id,
                user_id: r.user_id?,
                wallet_address: r.wallet_address,
                kwh_amount: r.kwh_amount?,
                reading_timestamp: r.reading_timestamp?,
                submitted_at: r.submitted_at?,
                minted: r.minted.unwrap_or(false),
                mint_tx_signature: r.mint_tx_signature,
            })
        })
        .collect();

    // Create pagination metadata
    let sort_order_util = match query.sort_order {
        SortOrder::Asc => crate::utils::SortOrder::Asc,
        SortOrder::Desc => crate::utils::SortOrder::Desc,
    };
    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: query.page,
            page_size: query.per_page,
            sort_by: Some(query.sort_by.clone()),
            sort_order: sort_order_util,
        },
        total,
    );

    Ok(Json(MeterReadingsResponse { data, pagination }))
}

/// Get meter readings by wallet address
/// GET /api/meters/readings/:wallet_address
#[utoipa::path(
    get,
    path = "/api/meters/readings/{wallet_address}",
    tag = "meters",
    params(
        ("wallet_address" = String, Path, description = "Solana wallet address"),
        GetReadingsQuery
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Meter readings for specified wallet", body = Vec<MeterReadingResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_readings_by_wallet(
    State(state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
    Path(wallet_address): Path<String>,
    Query(mut query): Query<GetReadingsQuery>,
) -> Result<Json<MeterReadingsResponse>> {
    info!("Fetching readings for wallet: {}", wallet_address);

    // Validate query parameters
    query.validate()?;

    let limit = query.limit();
    let offset = query.offset();
    let sort_by = query.get_sort_field();
    let sort_order = query.sort_direction();

    // Get total count
    let total = state
        .meter_service
        .count_wallet_readings(&wallet_address, query.minted)
        .await
        .map_err(|e| {
            error!("Failed to count wallet readings: {}", e);
            ApiError::Internal(format!("Failed to count readings: {}", e))
        })?;

    // Get readings with pagination
    let readings = state
        .meter_service
        .get_readings_by_wallet(
            &wallet_address,
            limit,
            offset,
            sort_by,
            sort_order,
            query.minted,
        )
        .await
        .map_err(|e| {
            error!("Failed to fetch wallet readings: {}", e);
            ApiError::Internal(format!("Failed to fetch readings: {}", e))
        })?;

    let data: Vec<MeterReadingResponse> = readings
        .into_iter()
        .filter_map(|r| {
            // Only include readings with all required fields
            Some(MeterReadingResponse {
                id: r.id,
                user_id: r.user_id?,
                wallet_address: r.wallet_address,
                kwh_amount: r.kwh_amount?,
                reading_timestamp: r.reading_timestamp?,
                submitted_at: r.submitted_at?,
                minted: r.minted.unwrap_or(false),
                mint_tx_signature: r.mint_tx_signature,
            })
        })
        .collect();

    // Create pagination metadata
    let sort_order_util = match query.sort_order {
        SortOrder::Asc => crate::utils::SortOrder::Asc,
        SortOrder::Desc => crate::utils::SortOrder::Desc,
    };
    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: query.page,
            page_size: query.per_page,
            sort_by: Some(query.sort_by.clone()),
            sort_order: sort_order_util,
        },
        total,
    );

    Ok(Json(MeterReadingsResponse { data, pagination }))
}

/// Get unminted readings (admin only)
/// GET /api/admin/meters/unminted
#[utoipa::path(
    get,
    path = "/api/admin/meters/unminted",
    tag = "meters",
    params(GetReadingsQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of unminted meter readings", body = Vec<MeterReadingResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_unminted_readings(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(mut query): Query<GetReadingsQuery>,
) -> Result<Json<Vec<MeterReadingResponse>>> {
    // Check admin permission
    require_role(&user, "admin")?;

    info!("Admin {} fetching unminted readings", user.sub);

    // Validate query parameters
    query.validate()?;

    let readings = state
        .meter_service
        .get_unminted_readings(query.limit())
        .await
        .map_err(|e| {
            error!("Failed to fetch unminted readings: {}", e);
            ApiError::Internal(format!("Failed to fetch readings: {}", e))
        })?;

    let response: Vec<MeterReadingResponse> = readings
        .into_iter()
        .filter_map(|r| {
            // Only include readings with all required fields
            Some(MeterReadingResponse {
                id: r.id,
                user_id: r.user_id?,
                wallet_address: r.wallet_address,
                kwh_amount: r.kwh_amount?,
                reading_timestamp: r.reading_timestamp?,
                submitted_at: r.submitted_at?,
                minted: r.minted.unwrap_or(false),
                mint_tx_signature: r.mint_tx_signature,
            })
        })
        .collect();

    Ok(Json(response))
}

/// Mint tokens from a meter reading (admin only)
/// POST /api/admin/meters/mint-from-reading
///
/// This endpoint mints energy tokens based on a submitted meter reading
#[utoipa::path(
    post,
    path = "/api/admin/meters/mint-from-reading",
    tag = "meters",
    request_body = MintFromReadingRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Tokens minted successfully", body = MintResponse),
        (status = 400, description = "Invalid request or reading already minted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "Reading not found"),
        (status = 500, description = "Internal server error or blockchain minting failed")
    )
)]
pub async fn mint_from_reading(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<MintFromReadingRequest>,
) -> Result<Json<MintResponse>> {
    // Check admin permission
    require_role(&user, "admin")?;

    info!(
        "Admin {} minting tokens for reading {}",
        user.sub, request.reading_id
    );

    // Get the reading
    let reading = state
        .meter_service
        .get_reading_by_id(request.reading_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch reading: {}", e);
            ApiError::NotFound(format!("Reading not found: {}", e))
        })?;

    // Check if already minted
    if reading.minted.unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "Reading has already been minted".to_string(),
        ));
    }

    // Parse wallet address
    let wallet_pubkey = BlockchainService::parse_pubkey(&reading.wallet_address)
        .map_err(|e| ApiError::BadRequest(format!("Invalid wallet address: {}", e)))?;

    // Get authority keypair
    let authority_keypair = state
        .wallet_service
        .get_authority_keypair()
        .await
        .map_err(|e| {
            error!("Failed to get authority keypair: {}", e);
            ApiError::Internal("Authority wallet not configured".to_string())
        })?;

    // Get token mint address from config
    let token_mint = BlockchainService::parse_pubkey(&state.config.energy_token_mint)
        .map_err(|e| ApiError::Internal(format!("Invalid token mint config: {}", e)))?;

    // Ensure user has token account (create if needed)
    let user_token_account = state
        .blockchain_service
        .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to create token account: {}", e)))?;

    info!("User token account: {}", user_token_account);

    // Mint tokens on blockchain
    let kwh_amount = reading
        .kwh_amount
        .ok_or_else(|| ApiError::Internal("Missing kWh amount".to_string()))?;

    let amount_kwh = kwh_amount
        .to_string()
        .parse::<f64>()
        .map_err(|e| ApiError::Internal(format!("Invalid kWh amount: {}", e)))?;

    let tx_signature = state
        .blockchain_service
        .mint_energy_tokens(
            &authority_keypair,
            &user_token_account,
            &wallet_pubkey,
            &token_mint,
            amount_kwh,
        )
        .await
        .map_err(|e| {
            error!("Failed to mint tokens on blockchain: {}", e);
            ApiError::Internal(format!("Blockchain minting failed: {}", e))
        })?;

    let tx_signature_string = tx_signature.to_string();
    info!("Tokens minted successfully. TX: {}", tx_signature_string);

    // Mark reading as minted
    let updated_reading = state
        .meter_service
        .mark_as_minted(reading.id, &tx_signature_string)
        .await
        .map_err(|e| {
            error!("Failed to mark reading as minted: {}", e);
            ApiError::Internal(format!("Failed to update reading: {}", e))
        })?;

    info!(
        "Successfully minted tokens for reading {}. TX: {}",
        reading.id, tx_signature_string
    );

    Ok(Json(MintResponse {
        message: "Tokens minted successfully".to_string(),
        transaction_signature: tx_signature.to_string(),
        kwh_amount: updated_reading
            .kwh_amount
            .ok_or_else(|| ApiError::Internal("Missing kWh amount".to_string()))?,
        wallet_address: reading.wallet_address,
    }))
}

/// Get user statistics
/// GET /api/meters/stats
#[utoipa::path(
    get,
    path = "/api/meters/stats",
    tag = "meters",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User meter reading statistics", body = UserStatsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_stats(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<UserStatsResponse>> {
    info!("User {} fetching meter statistics", user.sub);

    // Get unminted and minted totals
    let unminted_total = state
        .meter_service
        .get_unminted_total(user.sub)
        .await
        .map_err(|e| {
            error!("Failed to calculate unminted total: {}", e);
            ApiError::Internal("Failed to fetch statistics".to_string())
        })?;

    let minted_total = state
        .meter_service
        .get_minted_total(user.sub)
        .await
        .map_err(|e| {
            error!("Failed to calculate minted total: {}", e);
            ApiError::Internal("Failed to fetch statistics".to_string())
        })?;

    let total_kwh = unminted_total + minted_total;

    // Count total readings
    let total_readings = state
        .meter_service
        .count_user_readings(user.sub, None)
        .await
        .map_err(|e| {
            error!("Failed to count user readings: {}", e);
            ApiError::Internal("Failed to fetch statistics".to_string())
        })?;

    Ok(Json(UserStatsResponse {
        total_readings,
        unminted_kwh: unminted_total,
        minted_kwh: minted_total,
        total_kwh,
    }))
}
