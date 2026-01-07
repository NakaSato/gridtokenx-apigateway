use axum::{
    extract::{Query, State},
    response::Json,
};
use utoipa::{IntoParams, ToSchema};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::trading::{TradingOrder, TradingOrderDb};
use crate::utils::PaginationParams;
use crate::AppState;

use crate::handlers::trading::types::{OrderQuery, TradingOrdersResponse};

/// Get user's trading orders
/// GET /api/trading/orders
#[utoipa::path(
    get,
    path = "/api/trading/orders",
    tag = "trading",
    params(OrderQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's trading orders", body = Vec<TradingOrder>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user_orders(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
    Query(mut params): Query<OrderQuery>,
) -> Result<Json<TradingOrdersResponse>> {
    tracing::info!("Fetching orders for user: {}", user.0.sub);

    // Validate parameters
    params.validate_params()?;

    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build dynamic query based on parameters
    let mut where_conditions = vec!["user_id = $1".to_string()];
    let mut bind_count = 2;

    if params.status.is_some() {
        where_conditions.push(format!("status = ${}", bind_count));
        bind_count += 1;
    }

    if params.side.is_some() {
        where_conditions.push(format!("side = ${}", bind_count));
        bind_count += 1;
    }

    if params.order_type.is_some() {
        where_conditions.push(format!("order_type = ${}", bind_count));
        bind_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM trading_orders WHERE {}", where_clause);
    let mut count_sqlx = sqlx::query_scalar::<_, i64>(&count_query);
    count_sqlx = count_sqlx.bind(user.0.sub);
    if let Some(status) = &params.status {
        count_sqlx = count_sqlx.bind(status);
    }
    if let Some(side) = &params.side {
        count_sqlx = count_sqlx.bind(side);
    }
    if let Some(order_type) = &params.order_type {
        count_sqlx = count_sqlx.bind(order_type);
    }

    let total = count_sqlx.fetch_one(&_state.db).await.map_err(|e| {
        tracing::error!("Failed to count trading orders: {}", e);
        ApiError::Database(e)
    })?;

    // Build data query with sorting
    let query = format!(
        "SELECT id, user_id, order_type, side, energy_amount, price_per_kwh, filled_amount, status, expires_at, created_at, filled_at, epoch_id, zone_id 
         FROM trading_orders 
         WHERE {} 
         ORDER BY {} {}
         LIMIT ${} OFFSET ${}",
        where_clause, sort_field, sort_direction, bind_count, bind_count + 1
    );

    // Execute parameterized query
    let mut sqlx_query = sqlx::query_as::<_, TradingOrderDb>(&query);
    sqlx_query = sqlx_query.bind(user.0.sub);

    if let Some(status) = &params.status {
        sqlx_query = sqlx_query.bind(status);
    }
    if let Some(side) = &params.side {
        sqlx_query = sqlx_query.bind(side);
    }
    if let Some(order_type) = &params.order_type {
        sqlx_query = sqlx_query.bind(order_type);
    }

    sqlx_query = sqlx_query.bind(limit);
    sqlx_query = sqlx_query.bind(offset);

    let orders = sqlx_query
        .fetch_all(&_state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch trading orders: {}", e);
            ApiError::Database(e)
        })?
        .into_iter()
        .map(|db_order| db_order.into())
        .collect::<Vec<TradingOrder>>();

    // Create pagination metadata
    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    Ok(Json(TradingOrdersResponse {
        data: orders,
        pagination,
    }))
}

/// Get public order book
/// GET /api/trading/orderbook
#[utoipa::path(
    get,
    path = "/api/trading/orderbook",
    tag = "trading",
    params(OrderQuery),
    responses(
        (status = 200, description = "Public order book", body = Vec<TradingOrder>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_order_book(
    State(_state): State<AppState>,
    Query(mut params): Query<OrderQuery>,
) -> Result<Json<TradingOrdersResponse>> {
    tracing::info!("Fetching public order book");

    params.validate_params()?;

    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build dynamic query - include Pending and Active orders for matching
    // If a specific status is requested, use that; otherwise show both pending and active
    let mut where_conditions = vec!["expires_at > NOW()".to_string()];
    let mut bind_count = 1;

    if let Some(_status) = &params.status {
        where_conditions.push(format!("status = ${}", bind_count));
        bind_count += 1;
    } else {
        // Default: show both pending and active orders for order book
        where_conditions.push("(status = 'pending' OR status = 'active')".to_string());
    }

    if params.side.is_some() {
        where_conditions.push(format!("side = ${}", bind_count));
        bind_count += 1;
    }

    if params.order_type.is_some() {
        where_conditions.push(format!("order_type = ${}", bind_count));
        bind_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM trading_orders WHERE {}", where_clause);
    let mut count_sqlx = sqlx::query_scalar::<_, i64>(&count_query);
    
    if let Some(status) = &params.status {
        count_sqlx = count_sqlx.bind(status);
    }
    if let Some(side) = &params.side {
        count_sqlx = count_sqlx.bind(side);
    }
    if let Some(order_type) = &params.order_type {
        count_sqlx = count_sqlx.bind(order_type);
    }

    let total = count_sqlx.fetch_one(&_state.db).await.map_err(|e| {
        tracing::error!("Failed to count order book: {}", e);
        ApiError::Database(e)
    })?;

    // Build data query
    let query = format!(
        "SELECT id, user_id, order_type, side, energy_amount, price_per_kwh, filled_amount, status, expires_at, created_at, epoch_id, filled_at 
         FROM trading_orders 
         WHERE {} 
         ORDER BY {} {}
         LIMIT ${} OFFSET ${}",
        where_clause, sort_field, sort_direction, bind_count, bind_count + 1
    );

    let mut sqlx_query = sqlx::query_as::<_, TradingOrderDb>(&query);
    
    if let Some(status) = &params.status {
        sqlx_query = sqlx_query.bind(status);
    }
    if let Some(side) = &params.side {
        sqlx_query = sqlx_query.bind(side);
    }
    if let Some(order_type) = &params.order_type {
        sqlx_query = sqlx_query.bind(order_type);
    }

    sqlx_query = sqlx_query.bind(limit);
    sqlx_query = sqlx_query.bind(offset);

    let orders = sqlx_query
        .fetch_all(&_state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch order book: {}", e);
            ApiError::Database(e)
        })?
        .into_iter()
        .map(|db_order| db_order.into())
        .collect::<Vec<TradingOrder>>();

    let pagination = crate::utils::PaginationMeta::new(
        &PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    Ok(Json(TradingOrdersResponse {
        data: orders,
        pagination,
    }))
}

/// Get user's trade history (matches where they were buyer or seller)
/// GET /api/v1/trading/trades
#[utoipa::path(
    get,
    path = "/api/v1/trading/trades",
    tag = "trading",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of trades to return (default: 20)")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User's trade history"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_my_trades(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<TradeHistoryParams>,
) -> Result<Json<TradeHistoryResponse>> {
    tracing::info!("Fetching trade history for user: {}", user.0.sub);

    let limit = params.limit.unwrap_or(20).min(100);

    // Query order_matches where user was either buyer or seller
    let trades = sqlx::query_as::<_, TradeRecord>(
        r#"
        SELECT 
            om.id,
            om.matched_amount as quantity,
            om.match_price as price,
            (om.matched_amount * om.match_price) as total_value,
            CASE 
                WHEN buy_order.user_id = $1 THEN 'buyer'
                ELSE 'seller'
            END as role,
            CASE 
                WHEN buy_order.user_id = $1 THEN sell_order.user_id
                ELSE buy_order.user_id
            END as counterparty_id,
            om.match_time as executed_at,
            om.status,
            s.wheeling_charge,
            s.loss_cost,
            s.effective_energy,
            s.buyer_zone_id,
            s.seller_zone_id
        FROM order_matches om
        JOIN trading_orders buy_order ON om.buy_order_id = buy_order.id
        JOIN trading_orders sell_order ON om.sell_order_id = sell_order.id
        LEFT JOIN settlements s ON om.settlement_id = s.id
        WHERE buy_order.user_id = $1 OR sell_order.user_id = $1
        ORDER BY om.match_time DESC
        LIMIT $2
        "#,
    )
    .bind(user.0.sub)
    .bind(limit)
    .fetch_all(&_state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch trade history: {}", e);
        ApiError::Database(e)
    })?;

    Ok(Json(TradeHistoryResponse { trades }))
}

#[derive(Debug, serde::Deserialize, IntoParams)]
pub struct TradeHistoryParams {
    pub limit: Option<i32>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow, ToSchema)]
pub struct TradeRecord {
    pub id: uuid::Uuid,
    #[schema(value_type = String)]
    pub quantity: rust_decimal::Decimal,
    #[schema(value_type = String)]
    pub price: rust_decimal::Decimal,
    #[schema(value_type = String)]
    pub total_value: rust_decimal::Decimal,
    pub role: String,
    pub counterparty_id: uuid::Uuid,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    #[schema(value_type = Option<String>)]
    pub wheeling_charge: Option<rust_decimal::Decimal>,
    #[schema(value_type = Option<String>)]
    pub loss_cost: Option<rust_decimal::Decimal>,
    #[schema(value_type = Option<String>)]
    pub effective_energy: Option<rust_decimal::Decimal>,
    pub buyer_zone_id: Option<i32>,
    pub seller_zone_id: Option<i32>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct TradeHistoryResponse {
    pub trades: Vec<TradeRecord>,
}

/// Get user's GRID token balance
/// GET /api/v1/trading/balance
#[utoipa::path(
    get,
    path = "/api/v1/trading/balance",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User's token balance"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_token_balance(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<TokenBalanceResponse>> {
    tracing::info!("Fetching token balance for user: {}", user.0.sub);

    // Get user's wallet address from database
    let wallet_result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT wallet_address FROM users WHERE id = $1"
    )
    .bind(user.0.sub)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch user wallet: {}", e);
        ApiError::Database(e)
    })?;

    let wallet_address = match wallet_result {
        Some(addr) if !addr.is_empty() => addr,
        _ => {
            return Ok(Json(TokenBalanceResponse {
                wallet_address: None,
                token_balance: 0.0,
                raw_balance: 0,
                mint: state.config.energy_token_mint.clone(),
            }));
        }
    };

    // Parse wallet as Pubkey
    let wallet_pubkey = solana_sdk::pubkey::Pubkey::from_str(&wallet_address)
        .map_err(|e| {
            tracing::error!("Invalid wallet address: {}", e);
            ApiError::BadRequest(format!("Invalid wallet address: {}", e))
        })?;

    // Parse mint
    let mint_pubkey = solana_sdk::pubkey::Pubkey::from_str(&state.config.energy_token_mint)
        .map_err(|e| {
            tracing::error!("Invalid mint address: {}", e);
            ApiError::Internal(format!("Invalid mint address: {}", e))
        })?;

    // Get balance from blockchain
    let raw_balance = state
        .blockchain_service
        .get_token_balance(&wallet_pubkey, &mint_pubkey)
        .await
        .unwrap_or(0);

    // Convert from raw (9 decimals) to human-readable
    let token_balance = raw_balance as f64 / 1_000_000_000.0;

    Ok(Json(TokenBalanceResponse {
        wallet_address: Some(wallet_address),
        token_balance,
        raw_balance,
        mint: state.config.energy_token_mint.clone(),
    }))
}

use std::str::FromStr;

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct TokenBalanceResponse {
    pub wallet_address: Option<String>,
    pub token_balance: f64,
    pub raw_balance: u64,
    pub mint: String,
}
