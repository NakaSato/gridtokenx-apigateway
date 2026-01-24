use axum::{extract::State, response::Json};
use chrono::Utc;
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};
use rust_decimal::prelude::ToPrimitive;

use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::error::{ApiError, Result};
use crate::AppState;
use uuid::Uuid;

use super::types::{
    BlockchainMarketData, CreateBlockchainOrderRequest, CreateBlockchainOrderResponse,
    MatchOrdersResponse,
};

/// Get blockchain trading market data
/// GET /api/trading/market/blockchain
#[utoipa::path(
    get,
    path = "/api/trading/market/blockchain",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Blockchain market data from Solana", body = BlockchainMarketData),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Trading market not found on blockchain"),
        (status = 500, description = "Blockchain communication error")
    )
)]
pub async fn get_blockchain_market_data(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<BlockchainMarketData>> {
    info!("Fetching blockchain trading market data");

    // Get the Trading program ID
    let trading_program_id = _state
        .blockchain_service
        .trading_program_id()
        .map_err(|e| {
            error!("Failed to parse trading program ID: {}", e);
            ApiError::Internal(format!("Invalid program ID: {}", e))
        })?;

    // Derive the market PDA
    // Market PDA seeds: ["market"]
    let (market_pda, _bump) = Pubkey::find_program_address(&[b"market"], &trading_program_id);

    info!("Market PDA: {}", market_pda);

    // Check if the account exists
    let account_exists = _state
        .blockchain_service
        .account_exists(&market_pda)
        .await
        .map_err(|e| {
            error!("Failed to check if market account exists: {}", e);
            ApiError::Internal(format!("Blockchain error: {}", e))
        })?;

    if !account_exists {
        return Err(ApiError::NotFound(
            "Trading market not found on blockchain".to_string(),
        ));
    }

    // Get the account data
    let account_data = _state
        .blockchain_service
        .get_account_data(&market_pda)
        .await
        .map_err(|e| {
            error!("Failed to fetch market account data: {}", e);
            ApiError::Internal(format!("Failed to fetch account: {}", e))
        })?;

    // Deserialize the account data (skip 8-byte discriminator)
    if account_data.len() < 8 {
        return Err(ApiError::Internal("Invalid account data".to_string()));
    }

    let market_data = parse_market_data(&account_data[8..]).map_err(|e| {
        error!("Failed to parse market data: {}", e);
        ApiError::Internal(format!("Failed to parse account data: {}", e))
    })?;

    info!("Successfully fetched blockchain market data");
    Ok(Json(market_data))
}

/// Create order on blockchain
/// POST /api/trading/orders/blockchain
#[utoipa::path(
    post,
    path = "/api/trading/orders/blockchain",
    tag = "trading",
    request_body = CreateBlockchainOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Blockchain order created successfully", body = CreateBlockchainOrderResponse),
        (status = 400, description = "Invalid order parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Blockchain transaction failed")
    )
)]
pub async fn create_blockchain_order(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateBlockchainOrderRequest>,
) -> Result<Json<CreateBlockchainOrderResponse>> {
    info!(
        "Creating blockchain order from user {}: {} {} kWh at {} per kWh",
        user.0.sub, payload.order_type, payload.energy_amount, payload.price_per_kwh
    );

    // Validate order type
    if payload.order_type != "buy" && payload.order_type != "sell" {
        return Err(ApiError::BadRequest(
            "Order type must be 'buy' or 'sell'".to_string(),
        ));
    }

    // Validate amounts
    if payload.energy_amount == 0 {
        return Err(ApiError::BadRequest(
            "Energy amount must be positive".to_string(),
        ));
    }

    if payload.price_per_kwh == 0 {
        return Err(ApiError::BadRequest(
            "Price per kWh must be positive".to_string(),
        ));
    }

    info!(
        "Blockchain order created: {} {} kWh at {} per kWh",
        payload.order_type, payload.energy_amount, payload.price_per_kwh
    );

    // Create order in DB
    let order_id = Uuid::new_v4();
    let now = Utc::now();
    let epoch = _state
        .market_clearing
        .get_or_create_epoch(now)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get epoch: {}", e)))?;

    let side = match payload.order_type.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(ApiError::BadRequest("Invalid order type".into())),
    };

    // Convert u64 to Decimal
    let energy_amount = rust_decimal::Decimal::from(payload.energy_amount);
    let price = rust_decimal::Decimal::from(payload.price_per_kwh);

    sqlx::query!(
        r#"
        INSERT INTO trading_orders (
            id, user_id, order_type, side, energy_amount, price_per_kwh,
            filled_amount, status, expires_at, created_at, epoch_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
        order_id,
        user.0.sub,
        OrderType::Limit as OrderType,
        side as OrderSide,
        energy_amount,
        price,
        rust_decimal::Decimal::ZERO,
        OrderStatus::Pending as OrderStatus,
        now + chrono::Duration::days(1),
        now,
        epoch.id
    )
    .execute(&_state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    Ok(Json(CreateBlockchainOrderResponse {
        success: true,
        message: format!(
            "{} order created successfully for {} kWh at {} per kWh",
            payload.order_type, payload.energy_amount, payload.price_per_kwh
        ),
        order_type: payload.order_type,
        energy_amount: payload.energy_amount,
        price_per_kwh: payload.price_per_kwh,
        transaction_signature: None,
    }))
}

/// Trigger order matching on blockchain (admin only)
/// POST /api/admin/trading/match-orders
#[utoipa::path(
    post,
    path = "/api/admin/trading/match-orders",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order matching initiated successfully", body = MatchOrdersResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn match_blockchain_orders(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<MatchOrdersResponse>> {
    info!("Order matching request from user: {}", user.0.sub);

    // Check user role - only admins can trigger matching
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&_state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Internal("Failed to fetch user data".to_string())
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can trigger order matching".to_string(),
        ));
    }

    info!("Order matching initiated by admin {}", user.0.sub);

    // Trigger matching cycle
    let (matched_count, volume_decimal) = _state
        .market_clearing_engine
        .trigger_matching()
        .await
        .map_err(|e| {
            error!("Failed to execute matching cycle: {}", e);
            ApiError::Internal(format!("Matching failed: {}", e))
        })?;

    let total_volume = volume_decimal.to_f64().unwrap_or(0.0);

    Ok(Json(MatchOrdersResponse {
        success: true,
        message: "Order matching initiated successfully".to_string(),
        matched_orders: matched_count as u32,
        total_volume,
    }))
}

/// Parse market data from raw bytes
fn parse_market_data(data: &[u8]) -> Result<BlockchainMarketData> {
    // Market struct layout:
    // - authority: Pubkey (32 bytes)
    // - active_orders: u64 (8 bytes)
    // - total_volume: u64 (8 bytes)
    // - total_trades: u64 (8 bytes)
    // - created_at: i64 (8 bytes)
    // - clearing_enabled: bool (1 byte + padding)
    // - market_fee_bps: u16 (2 bytes + padding)

    if data.len() < 72 {
        return Err(ApiError::Internal("Market data too short".to_string()));
    }

    // Parse authority (first 32 bytes)
    let authority = Pubkey::try_from(&data[0..32])
        .map_err(|e| ApiError::Internal(format!("Invalid authority pubkey: {}", e)))?;

    // Parse active_orders (bytes 32-40)
    let active_orders = u64::from_le_bytes([
        data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
    ]);

    // Parse total_volume (bytes 40-48)
    let total_volume = u64::from_le_bytes([
        data[40], data[41], data[42], data[43], data[44], data[45], data[46], data[47],
    ]);

    // Parse total_trades (bytes 48-56)
    let total_trades = u64::from_le_bytes([
        data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
    ]);

    // Parse created_at (bytes 56-64)
    let created_at = i64::from_le_bytes([
        data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
    ]);

    // Parse clearing_enabled (byte 64)
    let clearing_enabled = data[64] != 0;

    // Parse market_fee_bps (bytes 66-67, after 1 byte padding)
    let market_fee_bps = u16::from_le_bytes([data[66], data[67]]);

    Ok(BlockchainMarketData {
        authority: authority.to_string(),
        active_orders,
        total_volume,
        total_trades,
        market_fee_bps,
        clearing_enabled,
        created_at,
    })
}
