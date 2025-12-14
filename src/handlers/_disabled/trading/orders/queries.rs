use axum::{
    extract::{Query, State},
    response::Json,
};

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
        "SELECT id, user_id, order_type, side, energy_amount, price_per_kwh, filled_amount, status, expires_at, created_at, filled_at 
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

    // Default to Active status for order book if not specified
    if params.status.is_none() {
        params.status = Some(crate::database::schema::types::OrderStatus::Active);
    }

    params.validate_params()?;

    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build dynamic query
    let mut where_conditions = vec!["expires_at > NOW()".to_string()];
    let mut bind_count = 1;

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
