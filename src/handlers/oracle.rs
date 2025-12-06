use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};

/// Request to submit a price update (for future price oracle functionality)
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitPriceRequest {
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub timestamp: Option<i64>,
}

/// Response for price submission
#[derive(Debug, Serialize, ToSchema)]
pub struct PriceSubmissionResponse {
    pub success: bool,
    pub message: String,
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub timestamp: i64,
}

/// Current price data
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentPriceData {
    pub energy_type: String,
    pub price_per_kwh: f64,
    pub last_updated: i64,
    pub source: String,
}

/// Oracle data from blockchain
#[derive(Debug, Serialize, ToSchema)]
pub struct OracleDataResponse {
    pub authority: String,
    pub api_gateway: String,
    pub total_readings: u64,
    pub last_reading_timestamp: i64,
    pub last_clearing: i64,
    pub active: bool,
    pub created_at: i64,
}

/// Submit price data to oracle
/// POST /api/oracle/prices
#[utoipa::path(
    post,
    path = "/api/oracle/prices",
    tag = "oracle",
    request_body = SubmitPriceRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Price data submitted successfully", body = PriceSubmissionResponse),
        (status = 400, description = "Invalid price or energy type"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_price(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<SubmitPriceRequest>,
) -> Result<Json<PriceSubmissionResponse>> {
    info!(
        "Price submission request from user {}: {} at {} per kWh",
        user.0.sub, payload.energy_type, payload.price_per_kwh
    );

    // Validate price is positive
    if payload.price_per_kwh <= 0.0 {
        return Err(ApiError::BadRequest("Price must be positive".to_string()));
    }

    // Validate energy type
    let valid_types = vec!["solar", "wind", "battery", "grid"];
    if !valid_types.contains(&payload.energy_type.to_lowercase().as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Invalid energy type. Must be one of: {:?}",
            valid_types
        )));
    }

    // Check user role - only admins can submit prices
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Internal("Failed to fetch user data".to_string())
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can submit price data".to_string(),
        ));
    }

    let timestamp = payload
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    info!(
        "Price submitted: {} = ${} per kWh at timestamp {}",
        payload.energy_type, payload.price_per_kwh, timestamp
    );

    Ok(Json(PriceSubmissionResponse {
        success: true,
        message: "Price data submitted successfully".to_string(),
        energy_type: payload.energy_type,
        price_per_kwh: payload.price_per_kwh,
        timestamp,
    }))
}

/// Get current price data
/// GET /api/oracle/prices/current
#[utoipa::path(
    get,
    path = "/api/oracle/prices/current",
    tag = "oracle",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current energy prices", body = Vec<CurrentPriceData>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_current_prices(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<CurrentPriceData>>> {
    info!("Fetching current energy prices");

    let prices = vec![
        CurrentPriceData {
            energy_type: "solar".to_string(),
            price_per_kwh: 0.12,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "wind".to_string(),
            price_per_kwh: 0.10,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "battery".to_string(),
            price_per_kwh: 0.15,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "grid".to_string(),
            price_per_kwh: 0.13,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
    ];

    Ok(Json(prices))
}

/// Get oracle data from blockchain
/// GET /api/oracle/data
#[utoipa::path(
    get,
    path = "/api/oracle/data",
    tag = "oracle",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Oracle blockchain data", body = OracleDataResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Oracle data not found on blockchain"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_oracle_data(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<OracleDataResponse>> {
    info!("Fetching oracle data from blockchain");

    // Get the Oracle program ID
    let oracle_program_id =
        state.blockchain_service.oracle_program_id().map_err(|e| {
            error!("Failed to parse oracle program ID: {}", e);
            ApiError::Internal(format!("Invalid program ID: {}", e))
        })?;

    // Derive the oracle data PDA
    // Oracle data PDA seeds: ["oracle_data"]
    let (oracle_pda, _bump) = Pubkey::find_program_address(&[b"oracle_data"], &oracle_program_id);

    info!("Oracle PDA: {}", oracle_pda);

    // Check if the account exists
    let account_exists = state
        .blockchain_service
        .account_exists(&oracle_pda)
        .await
        .map_err(|e| {
            error!("Failed to check if oracle account exists: {}", e);
            ApiError::Internal(format!("Blockchain error: {}", e))
        })?;

    if !account_exists {
        return Err(ApiError::NotFound(
            "Oracle data account not found on blockchain".to_string(),
        ));
    }

    // Get the account data
    let account_data = state
        .blockchain_service
        .get_account_data(&oracle_pda)
        .await
        .map_err(|e| {
            error!("Failed to fetch oracle account data: {}", e);
            ApiError::Internal(format!("Failed to fetch account: {}", e))
        })?;

    // Deserialize the account data (skip 8-byte discriminator)
    if account_data.len() < 8 {
        return Err(ApiError::Internal("Invalid account data".to_string()));
    }

    let oracle_data = parse_oracle_data(&account_data[8..]).map_err(|e| {
        error!("Failed to parse oracle data: {}", e);
        ApiError::Internal(format!("Failed to parse account data: {}", e))
    })?;

    info!("Successfully fetched oracle data");
    Ok(Json(oracle_data))
}

/// Parse oracle data from raw bytes
fn parse_oracle_data(data: &[u8]) -> Result<OracleDataResponse> {
    // OracleData struct layout:
    // - authority: Pubkey (32 bytes)
    // - api_gateway: Pubkey (32 bytes)
    // - total_readings: u64 (8 bytes)
    // - last_reading_timestamp: i64 (8 bytes)
    // - last_clearing: i64 (8 bytes)
    // - active: bool (1 byte + padding)
    // - created_at: i64 (8 bytes)

    if data.len() < 97 {
        return Err(ApiError::Internal("Oracle data too short".to_string()));
    }

    // Parse authority (first 32 bytes)
    let authority = Pubkey::try_from(&data[0..32])
        .map_err(|e| ApiError::Internal(format!("Invalid authority pubkey: {}", e)))?;

    // Parse api_gateway (bytes 32-64)
    let api_gateway = Pubkey::try_from(&data[32..64])
        .map_err(|e| ApiError::Internal(format!("Invalid api_gateway pubkey: {}", e)))?;

    // Parse total_readings (bytes 64-72)
    let total_readings = u64::from_le_bytes([
        data[64], data[65], data[66], data[67], data[68], data[69], data[70], data[71],
    ]);

    // Parse last_reading_timestamp (bytes 72-80)
    let last_reading_timestamp = i64::from_le_bytes([
        data[72], data[73], data[74], data[75], data[76], data[77], data[78], data[79],
    ]);

    // Parse last_clearing (bytes 80-88)
    let last_clearing = i64::from_le_bytes([
        data[80], data[81], data[82], data[83], data[84], data[85], data[86], data[87],
    ]);

    // Parse active (byte 88)
    let active = data[88] != 0;

    // Parse created_at (bytes 96-104, after padding)
    let created_at = if data.len() >= 104 {
        i64::from_le_bytes([
            data[96], data[97], data[98], data[99], data[100], data[101], data[102], data[103],
        ])
    } else {
        0
    };

    Ok(OracleDataResponse {
        authority: authority.to_string(),
        api_gateway: api_gateway.to_string(),
        total_readings,
        last_reading_timestamp,
        last_clearing,
        active,
        created_at,
    })
}
