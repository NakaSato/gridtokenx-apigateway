use axum::{extract::State, response::Json};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::AppState;

/// Market health status
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketHealth {
    pub status: String,
    pub order_book_health: OrderBookHealth,
    pub matching_stats: MatchingStatistics,
    pub settlement_stats: SettlementStatistics,
    pub websocket_connections: usize,
    pub timestamp: String,
}

/// Order book health metrics
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookHealth {
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub spread: Option<String>,
    pub spread_percentage: Option<f64>,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
    pub total_buy_volume: String,
    pub total_sell_volume: String,
    pub liquidity_score: f64,
}

/// Matching statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchingStatistics {
    pub total_matches_24h: i64,
    pub total_volume_24h: String,
    pub average_price_24h: String,
    pub last_match_time: Option<String>,
    pub pending_orders: i64,
}

/// Settlement statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct SettlementStatistics {
    pub pending_count: i64,
    pub processing_count: i64,
    pub confirmed_count: i64,
    pub failed_count: i64,
    pub total_settled_value: String,
}

/// Market control request
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarketControlRequest {
    pub action: MarketAction,
}

/// Market actions
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarketAction {
    PauseTrading,
    ResumeTrading,
    ClearOrderBook,
    TriggerMatching,
}

/// Market control response
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketControlResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: String,
}

/// Get comprehensive market health status
#[utoipa::path(
    get,
    path = "/api/admin/market/health",
    responses(
        (status = 200, description = "Market health retrieved", body = MarketHealth),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn get_market_health(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketHealth>, ApiError> {
    // Get order book snapshot
    let order_book = state.market_clearing_engine.get_order_book_snapshot().await;

    // Calculate liquidity score (0-100)
    let total_buy: rust_decimal::Decimal = order_book.buy_depth.iter().map(|(_, v)| v).sum();
    let total_sell: rust_decimal::Decimal = order_book.sell_depth.iter().map(|(_, v)| v).sum();
    let total_liquidity = total_buy + total_sell;
    let liquidity_score = if total_liquidity > rust_decimal::Decimal::ZERO {
        ((total_liquidity.to_string().parse::<f64>().unwrap_or(0.0) / 1000.0) * 100.0).min(100.0)
    } else {
        0.0
    };

    let spread_percentage = match (&order_book.best_bid, &order_book.best_ask) {
        (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => Some(
            ((*ask - *bid) / *bid * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
        ),
        _ => None,
    };

    // Get matching statistics
    let matching_stats_row = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_matches,
            COALESCE(SUM(quantity::numeric), 0) as total_volume,
            COALESCE(AVG(price::numeric), 0) as average_price,
            MAX(executed_at) as last_match_time
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let pending_orders = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) 
        FROM trading_orders 
        WHERE status IN ('Pending', 'PartiallyFilled')
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Get settlement statistics (if settlement service exists)
    let settlement_stats = SettlementStatistics {
        pending_count: 0,
        processing_count: 0,
        confirmed_count: 0,
        failed_count: 0,
        total_settled_value: "0".to_string(),
    };

    // Get WebSocket connections
    let ws_connections = state.websocket_service.client_count().await;

    // Determine overall status
    let status = if liquidity_score > 50.0 && ws_connections > 0 {
        "healthy"
    } else if liquidity_score > 20.0 {
        "degraded"
    } else {
        "critical"
    };

    Ok(Json(MarketHealth {
        status: status.to_string(),
        order_book_health: OrderBookHealth {
            best_bid: order_book.best_bid.map(|p| p.to_string()),
            best_ask: order_book.best_ask.map(|p| p.to_string()),
            spread: order_book.spread.map(|p| p.to_string()),
            spread_percentage,
            buy_orders_count: order_book.buy_depth.len(),
            sell_orders_count: order_book.sell_depth.len(),
            total_buy_volume: total_buy.to_string(),
            total_sell_volume: total_sell.to_string(),
            liquidity_score,
        },
        matching_stats: MatchingStatistics {
            total_matches_24h: matching_stats_row
                .try_get::<i64, _>("total_matches")
                .unwrap_or(0),
            total_volume_24h: matching_stats_row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
            average_price_24h: matching_stats_row
                .try_get::<Decimal, _>("average_price")
                .unwrap_or_default()
                .to_string(),
            last_match_time: matching_stats_row
                .try_get::<Option<chrono::DateTime<Utc>>, _>("last_match_time")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
            pending_orders,
        },
        settlement_stats,
        websocket_connections: ws_connections,
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// Execute market control actions (admin only)
#[utoipa::path(
    post,
    path = "/api/admin/market/control",
    request_body = MarketControlRequest,
    responses(
        (status = 200, description = "Market control action executed", body = MarketControlResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn market_control(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<MarketControlRequest>,
) -> Result<Json<MarketControlResponse>, ApiError> {
    let result = match request.action {
        MarketAction::TriggerMatching => {
            // Manually trigger a matching cycle
            match state.market_clearing_engine.execute_matching_cycle().await {
                Ok(count) => MarketControlResponse {
                    success: true,
                    message: format!("Matching cycle completed: {} trades executed", count),
                    timestamp: Utc::now().to_rfc3339(),
                },
                Err(e) => MarketControlResponse {
                    success: false,
                    message: format!("Matching cycle failed: {}", e),
                    timestamp: Utc::now().to_rfc3339(),
                },
            }
        }
        MarketAction::PauseTrading => {
            // Note: This would require additional state management
            MarketControlResponse {
                success: true,
                message: "Trading paused (feature not yet implemented)".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }
        }
        MarketAction::ResumeTrading => MarketControlResponse {
            success: true,
            message: "Trading resumed (feature not yet implemented)".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
        MarketAction::ClearOrderBook => MarketControlResponse {
            success: false,
            message:
                "Clear order book is a dangerous operation and requires additional confirmation"
                    .to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
    };

    Ok(Json(result))
}
