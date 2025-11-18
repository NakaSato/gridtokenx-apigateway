// Market data and trading endpoints for the market clearing engine

use axum::{
    extract::State,
    response::Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::AppState;
use crate::services::ClearingPrice;

/// Market statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStats {
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub mid_price: Option<String>,
    pub spread: Option<String>,
    pub spread_percentage: Option<f64>,
    pub total_buy_volume: String,
    pub total_sell_volume: String,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
}

/// Order book depth response
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookDepth {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub mid_price: Option<String>,
    pub spread: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PriceLevel {
    pub price: String,
    pub volume: String,
}

/// Get current market statistics
#[utoipa::path(
    get,
    path = "/api/market/stats",
    responses(
        (status = 200, description = "Market statistics retrieved", body = MarketStats),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data"
)]
pub async fn get_market_stats(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketStats>, ApiError> {
    let snapshot = state.market_clearing_engine.get_order_book_snapshot().await;

    let total_buy_volume: rust_decimal::Decimal = snapshot.buy_depth.iter()
        .map(|(_, vol)| vol)
        .sum();
    
    let total_sell_volume: rust_decimal::Decimal = snapshot.sell_depth.iter()
        .map(|(_, vol)| vol)
        .sum();

    let spread_percentage = match (&snapshot.best_bid, &snapshot.best_ask) {
        (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => {
            Some(((*ask - *bid) / *bid * rust_decimal::Decimal::from(100)).to_string().parse::<f64>().unwrap_or(0.0))
        }
        _ => None,
    };

    Ok(Json(MarketStats {
        best_bid: snapshot.best_bid.map(|p| p.to_string()),
        best_ask: snapshot.best_ask.map(|p| p.to_string()),
        mid_price: snapshot.mid_price.map(|p| p.to_string()),
        spread: snapshot.spread.map(|p| p.to_string()),
        spread_percentage,
        total_buy_volume: total_buy_volume.to_string(),
        total_sell_volume: total_sell_volume.to_string(),
        buy_orders_count: snapshot.buy_depth.len(),
        sell_orders_count: snapshot.sell_depth.len(),
    }))
}

/// Get order book depth
#[utoipa::path(
    get,
    path = "/api/market/depth",
    responses(
        (status = 200, description = "Order book depth retrieved", body = OrderBookDepth),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data"
)]
pub async fn get_order_book_depth(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<OrderBookDepth>, ApiError> {
    let snapshot = state.market_clearing_engine.get_order_book_snapshot().await;

    let bids: Vec<PriceLevel> = snapshot.buy_depth.iter()
        .map(|(price, volume)| PriceLevel {
            price: price.to_string(),
            volume: volume.to_string(),
        })
        .collect();

    let asks: Vec<PriceLevel> = snapshot.sell_depth.iter()
        .map(|(price, volume)| PriceLevel {
            price: price.to_string(),
            volume: volume.to_string(),
        })
        .collect();

    Ok(Json(OrderBookDepth {
        bids,
        asks,
        mid_price: snapshot.mid_price.map(|p| p.to_string()),
        spread: snapshot.spread.map(|p| p.to_string()),
        timestamp: snapshot.timestamp.to_rfc3339(),
    }))
}

/// Get clearing price calculation
#[utoipa::path(
    get,
    path = "/api/market/clearing-price",
    responses(
        (status = 200, description = "Clearing price calculated", body = ClearingPrice),
        (status = 404, description = "Insufficient market depth"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data"
)]
pub async fn get_clearing_price(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<ClearingPrice>, ApiError> {
    let clearing = state.market_clearing_engine.calculate_clearing_price().await
        .ok_or(ApiError::NotFound("Insufficient market depth for clearing price calculation".into()))?;

    Ok(Json(clearing))
}

/// User's recent trades
#[derive(Debug, Serialize, ToSchema)]
pub struct TradeHistory {
    pub trades: Vec<TradeRecord>,
    pub total_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TradeRecord {
    pub id: String,
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub quantity: String,
    pub price: String,
    pub total_value: String,
    pub role: String,  // "buyer" or "seller"
    pub counterparty_id: String,
    pub executed_at: String,
    pub status: String,
}

/// Get user's trade history
#[utoipa::path(
    get,
    path = "/api/market/trades/my-history",
    responses(
        (status = 200, description = "Trade history retrieved", body = TradeHistory),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data",
    security(("bearer_auth" = []))
)]
pub async fn get_my_trade_history(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<TradeHistory>, ApiError> {
    let user_id = user.0.sub;

    let trades = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, String, String)>(
        r#"
        SELECT 
            id::text,
            buy_order_id::text,
            sell_order_id::text,
            buyer_id::text,
            seller_id::text,
            quantity::text,
            price::text,
            total_value::text,
            executed_at::text,
            status,
            CASE 
                WHEN buyer_id = $1 THEN 'buyer'
                ELSE 'seller'
            END as role
        FROM trades
        WHERE buyer_id = $1 OR seller_id = $1
        ORDER BY executed_at DESC
        LIMIT 50
        "#
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let total_count = trades.len() as i64;

    let trade_records: Vec<TradeRecord> = trades.into_iter().map(|(
        id, buy_order_id, sell_order_id, buyer_id, seller_id, 
        quantity, price, total_value, executed_at, status, role
    )| {
        let counterparty_id = if role == "buyer" { seller_id } else { buyer_id };
        
        TradeRecord {
            id,
            buy_order_id,
            sell_order_id,
            quantity,
            price,
            total_value,
            role,
            counterparty_id,
            executed_at,
            status,
        }
    }).collect();

    Ok(Json(TradeHistory {
        trades: trade_records,
        total_count,
    }))
}

/// Market depth chart data
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketDepthChart {
    pub cumulative_bids: Vec<DepthPoint>,
    pub cumulative_asks: Vec<DepthPoint>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DepthPoint {
    pub price: String,
    pub cumulative_volume: String,
}

/// Get market depth chart data (cumulative)
#[utoipa::path(
    get,
    path = "/api/market/depth-chart",
    responses(
        (status = 200, description = "Market depth chart data", body = MarketDepthChart),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data"
)]
pub async fn get_market_depth_chart(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketDepthChart>, ApiError> {
    let snapshot = state.market_clearing_engine.get_order_book_snapshot().await;

    // Build cumulative bid curve (highest to lowest price)
    let mut cumulative_volume = rust_decimal::Decimal::ZERO;
    let cumulative_bids: Vec<DepthPoint> = snapshot.buy_depth.iter()
        .map(|(price, volume)| {
            cumulative_volume += volume;
            DepthPoint {
                price: price.to_string(),
                cumulative_volume: cumulative_volume.to_string(),
            }
        })
        .collect();

    // Build cumulative ask curve (lowest to highest price)
    cumulative_volume = rust_decimal::Decimal::ZERO;
    let cumulative_asks: Vec<DepthPoint> = snapshot.sell_depth.iter()
        .map(|(price, volume)| {
            cumulative_volume += volume;
            DepthPoint {
                price: price.to_string(),
                cumulative_volume: cumulative_volume.to_string(),
            }
        })
        .collect();

    Ok(Json(MarketDepthChart {
        cumulative_bids,
        cumulative_asks,
    }))
}
