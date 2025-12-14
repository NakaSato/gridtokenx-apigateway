use axum::{extract::State, response::Json};

use super::types::{DepthPoint, MarketDepthChart, MarketStats, OrderBookDepth, PriceLevel};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::services::ClearingPrice;
use crate::AppState;

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

    let total_buy_volume: rust_decimal::Decimal =
        snapshot.buy_depth.iter().map(|(_, vol)| vol).sum();

    let total_sell_volume: rust_decimal::Decimal =
        snapshot.sell_depth.iter().map(|(_, vol)| vol).sum();

    let spread_percentage = match (&snapshot.best_bid, &snapshot.best_ask) {
        (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => Some(
            ((*ask - *bid) / *bid * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
        ),
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

    let bids: Vec<PriceLevel> = snapshot
        .buy_depth
        .iter()
        .map(|(price, volume)| PriceLevel {
            price: price.to_string(),
            volume: volume.to_string(),
        })
        .collect();

    let asks: Vec<PriceLevel> = snapshot
        .sell_depth
        .iter()
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
    let clearing = state
        .market_clearing_engine
        .calculate_clearing_price()
        .await
        .ok_or(ApiError::NotFound(
            "Insufficient market depth for clearing price calculation".into(),
        ))?;

    Ok(Json(clearing))
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
    let cumulative_bids: Vec<DepthPoint> = snapshot
        .buy_depth
        .iter()
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
    let cumulative_asks: Vec<DepthPoint> = snapshot
        .sell_depth
        .iter()
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
