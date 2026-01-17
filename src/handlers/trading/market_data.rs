use axum::{extract::State, response::Json};
use chrono::{DateTime, Utc};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::trading::{MarketData, OrderBook};
use crate::AppState;

use super::types::{MarketStats, TradingStats, OrderBookResponse};

/// Get current market data
/// GET /api/trading/market
#[utoipa::path(
    get,
    path = "/api/trading/market",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current market data including order book", body = MarketData),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_market_data(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketData>> {
    tracing::info!("Fetching current market data");

    // Get current epoch information (for now, use simple hour-based epochs)
    let now = Utc::now();
    let current_epoch = (now.timestamp() / 3600) as u64; // 1-hour epochs
    let epoch_start = DateTime::from_timestamp(current_epoch as i64 * 3600, 0)
        .ok_or_else(|| ApiError::Internal("Failed to create epoch start timestamp".to_string()))?;
    let epoch_end = epoch_start + chrono::Duration::hours(1);

    // For now, return basic market data structure
    // In Phase 4, this will include real order book and trade data
    let market_data = MarketData {
        current_epoch,
        epoch_start_time: epoch_start,
        epoch_end_time: epoch_end,
        status: "active".to_string(),
        order_book: OrderBook {
            sell_orders: vec![],
            buy_orders: vec![],
        },
        recent_trades: vec![],
    };

    Ok(Json(market_data))
}

/// Get trading statistics for user
/// GET /api/trading/stats
#[utoipa::path(
    get,
    path = "/api/trading/stats",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Trading statistics for authenticated user", body = TradingStats),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_trading_stats(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<TradingStats>> {
    tracing::info!("Fetching trading stats for user: {}", user.0.sub);

    // For now, return basic stats structure
    // In Phase 4, this will include real database queries
    let trading_stats = TradingStats {
        total_orders: 0,
        active_orders: 0,
        filled_orders: 0,
        cancelled_orders: 0,
    };

    Ok(Json(trading_stats))
}

/// Get order book (buy and sell orders)
#[utoipa::path(
    get,
    path = "/api/trading/orderbook",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order book data", body = OrderBookResponse),
    )
)]
pub async fn get_orderbook(State(state): State<AppState>) -> Result<Json<super::types::OrderBookResponse>> {
    use rust_decimal::Decimal;
    use sqlx::Row;
    use crate::services::cache::CacheKeys;

    const ORDERBOOK_CACHE_TTL: u64 = 10; // 10 seconds TTL for real-time data
    let cache_key = CacheKeys::order_book("default");

    // Try cache first
    if let Ok(Some(cached)) = state.cache_service.get_json::<super::types::OrderBookResponse>(&cache_key).await {
        tracing::debug!("Order book cache HIT");
        return Ok(Json(cached));
    }

    tracing::debug!("Order book cache MISS - fetching from DB");

    // Get buy orders
    let buy_orders = sqlx::query(
        r#"
        SELECT o.energy_amount, o.price_per_kwh, u.username
        FROM trading_orders o
        JOIN users u ON o.user_id = u.id
        WHERE o.side = 'buy' AND o.status = 'pending'
        ORDER BY o.price_per_kwh DESC, o.created_at ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    // Get sell orders
    let sell_orders = sqlx::query(
        r#"
        SELECT o.energy_amount, o.price_per_kwh, u.username
        FROM trading_orders o
        JOIN users u ON o.user_id = u.id
        WHERE o.side = 'sell' AND o.status = 'pending'
        ORDER BY o.price_per_kwh ASC, o.created_at ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let buys: Vec<super::types::OrderBookEntry> = buy_orders
        .iter()
        .map(|row| {
            let energy_amount: Decimal = row.get("energy_amount");
            let price_per_kwh: Decimal = row.get("price_per_kwh");
            super::types::OrderBookEntry {
                energy_amount: energy_amount.to_string().parse::<f64>().unwrap_or(0.0),
                price_per_kwh: price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0),
                username: row.get::<Option<String>, _>("username")
            }
        })
        .collect::<Vec<_>>();

    let sells: Vec<super::types::OrderBookEntry> = sell_orders
        .iter()
        .map(|row| {
            let energy_amount: Decimal = row.get("energy_amount");
            let price_per_kwh: Decimal = row.get("price_per_kwh");
            super::types::OrderBookEntry {
                energy_amount: energy_amount.to_string().parse::<f64>().unwrap_or(0.0),
                price_per_kwh: price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0),
                username: row.get::<Option<String>, _>("username")
            }
        })
        .collect::<Vec<_>>();

    let response = super::types::OrderBookResponse {
        buy_orders: buys,
        sell_orders: sells,
        timestamp: Utc::now(),
    };

    // Store in cache
    if let Err(e) = state.cache_service.set_with_ttl(&cache_key, &response, ORDERBOOK_CACHE_TTL).await {
        tracing::warn!("Failed to cache order book: {}", e);
    }

    Ok(Json(response))
}

/// Get market statistics
#[utoipa::path(
    get,
    path = "/api/trading/stats",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Market statistics", body = MarketStats),
    )
)]
pub async fn get_market_stats(
    State(state): State<AppState>,
) -> Result<Json<super::types::MarketStats>> {
    use rust_decimal::Decimal;
    use sqlx::Row;
    use crate::services::cache::CacheKeys;

    const MARKET_STATS_CACHE_TTL: u64 = 30; // 30 seconds TTL for aggregated stats
    let cache_key = CacheKeys::market_stats("24h");

    // Try cache first
    if let Ok(Some(cached)) = state.cache_service.get_json::<super::types::MarketStats>(&cache_key).await {
        tracing::debug!("Market stats cache HIT");
        return Ok(Json(cached));
    }

    tracing::debug!("Market stats cache MISS - fetching from DB");

    // Get average price and volume from recent matches
    let stats_row = sqlx::query(
        r#"
        SELECT
            COALESCE(AVG(match_price), 0) as avg_price,
            COALESCE(SUM(matched_amount), 0) as total_volume,
            COUNT(*) as completed_matches
        FROM order_matches
        WHERE created_at > NOW() - INTERVAL '24 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let avg_price: Decimal = stats_row.try_get("avg_price").unwrap_or(Decimal::ZERO);
    let total_volume: Decimal = stats_row.try_get("total_volume").unwrap_or(Decimal::ZERO);
    let completed_matches: i64 = stats_row.try_get("completed_matches").unwrap_or(0);

    // Get active orders count (orders that are not filled or cancelled)
    let active_orders_row =
        sqlx::query("SELECT COUNT(*) as count FROM trading_orders WHERE status::TEXT = 'pending' OR status::TEXT = 'partially_filled'")
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Database(e))?;
    let active_orders: i64 = active_orders_row.try_get("count").unwrap_or(0);

    // Get pending orders count (specifically pending)
    let pending_orders_row =
        sqlx::query("SELECT COUNT(*) as count FROM trading_orders WHERE status::TEXT = 'pending'")
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Database(e))?;
    let pending_orders: i64 = pending_orders_row.try_get("count").unwrap_or(0);

    let response = super::types::MarketStats {
        average_price: avg_price.to_string().parse().unwrap_or(0.0),
        total_volume: total_volume.to_string().parse().unwrap_or(0.0),
        active_orders,
        pending_orders,
        completed_matches,
    };

    // Store in cache
    if let Err(e) = state.cache_service.set_with_ttl(&cache_key, &response, MARKET_STATS_CACHE_TTL).await {
        tracing::warn!("Failed to cache market stats: {}", e);
    }

    Ok(Json(response))
}
