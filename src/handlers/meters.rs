use axum::{
    Json,
    extract::{Path, Query, State},
};
use bigdecimal::{BigDecimal, ToPrimitive};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
// use solana_sdk::{pubkey::Pubkey, signature::Keypair}; // Not used yet

use crate::{
    AppState,
    auth::middleware::AuthenticatedUser,
    error::ApiError,
    services::BlockchainService, // MeterService, WalletService used through state
    utils::PaginationParams,
};

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if user has required role
fn require_role(user: &crate::auth::Claims, required_role: &str) -> Result<(), ApiError> {
    if user.role != required_role {
        return Err(ApiError::Forbidden(format!(
            "Required role: {}",
            required_role
        )));
    }
    Ok(())
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitReadingRequest {
    #[schema(value_type = String)]
    pub kwh_amount: BigDecimal,
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
    pub kwh_amount: BigDecimal,
    pub reading_timestamp: chrono::DateTime<chrono::Utc>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct GetReadingsQuery {
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_page_size")]
    pub page_size: u32,

    /// Sort field: "submitted_at", "reading_timestamp", "kwh_amount"
    pub sort_by: Option<String>,

    /// Sort direction: "asc" or "desc"
    #[serde(default = "default_sort_order")]
    pub sort_order: crate::utils::SortOrder,

    /// Filter by minted status
    pub minted: Option<bool>,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

fn default_sort_order() -> crate::utils::SortOrder {
    crate::utils::SortOrder::Desc
}

impl GetReadingsQuery {
    pub fn validate(&mut self) -> Result<(), ApiError> {
        if self.page < 1 {
            self.page = 1;
        }

        if self.page_size < 1 {
            self.page_size = 20;
        } else if self.page_size > 100 {
            self.page_size = 100;
        }

        // Validate sort field
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "submitted_at" | "reading_timestamp" | "kwh_amount" => {}
                _ => {
                    return Err(ApiError::validation_error(
                        "Invalid sort_by field. Allowed values: submitted_at, reading_timestamp, kwh_amount",
                        Some("sort_by"),
                    ));
                }
            }
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
            crate::utils::SortOrder::Asc => "ASC",
            crate::utils::SortOrder::Desc => "DESC",
        }
    }

    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("submitted_at")
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
    pub kwh_amount: BigDecimal,
    pub wallet_address: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserStatsResponse {
    pub total_readings: i64,
    #[schema(value_type = String)]
    pub unminted_kwh: BigDecimal,
    #[schema(value_type = String)]
    pub minted_kwh: BigDecimal,
    #[schema(value_type = String)]
    pub total_kwh: BigDecimal,
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
) -> Result<Json<MeterReadingResponse>, ApiError> {
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

    // NEW: Verify meter ownership if meter_id is provided
    let verification_status = if let Some(meter_id) = request.meter_id {
        // Verify meter ownership
        let is_owner = state
            .meter_verification_service
            .verify_meter_ownership(&user.sub.to_string(), &meter_id)
            .await
            .map_err(|e| {
                error!("Failed to verify meter ownership: {}", e);
                ApiError::Internal(format!("Failed to verify meter ownership: {}", e))
            })?;

        if !is_owner {
            return Err(ApiError::Forbidden(
                "You do not own this meter or it is not verified".to_string(),
            ));
        }

        info!("Meter ownership verified for meter_id: {}", meter_id);
        "verified"
    } else {
        // Legacy support during grace period
        warn!(
            "User {} submitting reading without meter_id - using legacy_unverified status",
            user.sub
        );
        "legacy_unverified"
    };

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
            user.sub,
            meter_request,
            request.meter_id,
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

    Ok(Json(MeterReadingResponse {
        id: reading.id,
        user_id: reading
            .user_id
            .ok_or_else(|| ApiError::Internal("Missing user_id".to_string()))?,
        wallet_address: reading.wallet_address,
        kwh_amount: reading
            .kwh_amount
            .ok_or_else(|| ApiError::Internal("Missing kwh_amount".to_string()))?,
        reading_timestamp: reading
            .reading_timestamp
            .ok_or_else(|| ApiError::Internal("Missing reading_timestamp".to_string()))?,
        submitted_at: reading
            .submitted_at
            .ok_or_else(|| ApiError::Internal("Missing submitted_at".to_string()))?,
        minted: reading.minted.unwrap_or(false),
        mint_tx_signature: reading.mint_tx_signature,
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
) -> Result<Json<MeterReadingsResponse>, ApiError> {
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
    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_order: query.sort_order,
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
) -> Result<Json<MeterReadingsResponse>, ApiError> {
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
    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: query.page,
            page_size: query.page_size,
            sort_by: query.sort_by.clone(),
            sort_order: query.sort_order,
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
) -> Result<Json<Vec<MeterReadingResponse>>, ApiError> {
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
) -> Result<Json<MintResponse>, ApiError> {
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
) -> Result<Json<UserStatsResponse>, ApiError> {
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

    let total_kwh = &unminted_total + &minted_total;

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
