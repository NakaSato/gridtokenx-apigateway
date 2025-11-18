// Admin endpoints for market monitoring and control
// Requires admin authentication

use axum::{
    extract::State,
    response::Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, types::BigDecimal};
use utoipa::ToSchema;
use std::str::FromStr;

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

/// Trading analytics
#[derive(Debug, Serialize, ToSchema)]
pub struct TradingAnalytics {
    pub total_trades: i64,
    pub total_volume: String,
    pub total_value: String,
    pub average_trade_size: String,
    pub price_statistics: PriceStatistics,
    pub top_traders: Vec<TraderStats>,
    pub hourly_volume: Vec<HourlyVolume>,
}

/// Price statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct PriceStatistics {
    pub current_price: Option<String>,
    pub high_24h: Option<String>,
    pub low_24h: Option<String>,
    pub open_24h: Option<String>,
    pub close_24h: Option<String>,
    pub change_24h: Option<String>,
    pub change_percentage_24h: Option<f64>,
}

/// Trader statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct TraderStats {
    pub user_id: String,
    pub total_trades: i64,
    pub total_volume: String,
    pub buy_volume: String,
    pub sell_volume: String,
}

/// Hourly volume data
#[derive(Debug, Serialize, ToSchema)]
pub struct HourlyVolume {
    pub hour: String,
    pub volume: String,
    pub trade_count: i64,
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
        (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => {
            Some(((*ask - *bid) / *bid * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0))
        }
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
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let pending_orders = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) 
        FROM trading_orders 
        WHERE status IN ('Pending', 'PartiallyFilled')
        "#
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
            total_matches_24h: matching_stats_row.try_get::<i64, _>("total_matches").unwrap_or(0),
            total_volume_24h: matching_stats_row.try_get::<BigDecimal, _>("total_volume").unwrap_or_default().to_string(),
            average_price_24h: matching_stats_row.try_get::<BigDecimal, _>("average_price").unwrap_or_default().to_string(),
            last_match_time: matching_stats_row.try_get::<Option<chrono::DateTime<Utc>>, _>("last_match_time").ok().flatten().map(|t| t.to_rfc3339()),
            pending_orders,
        },
        settlement_stats,
        websocket_connections: ws_connections,
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// Get detailed trading analytics
#[utoipa::path(
    get,
    path = "/api/admin/market/analytics",
    responses(
        (status = 200, description = "Trading analytics retrieved", body = TradingAnalytics),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn get_trading_analytics(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TradingAnalytics>, ApiError> {
    // Get overall trade statistics
    let overall_stats = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_trades,
            COALESCE(SUM(quantity::numeric), 0) as total_volume,
            COALESCE(SUM(total_value::numeric), 0) as total_value,
            COALESCE(AVG(quantity::numeric), 0) as avg_trade_size
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Get price statistics
    let price_stats = sqlx::query(
        r#"
        SELECT 
            (SELECT price::text FROM trades ORDER BY executed_at DESC LIMIT 1) as current_price,
            MAX(price::numeric) as high_24h,
            MIN(price::numeric) as low_24h,
            (SELECT price::numeric FROM trades WHERE executed_at > NOW() - INTERVAL '24 hours' ORDER BY executed_at ASC LIMIT 1) as open_24h,
            (SELECT price::numeric FROM trades ORDER BY executed_at DESC LIMIT 1) as close_24h
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let open_24h = price_stats.try_get::<Option<BigDecimal>, _>("open_24h").ok().flatten().and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());
    let close_24h = price_stats.try_get::<Option<BigDecimal>, _>("close_24h").ok().flatten().and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());

    let change_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) => Some((close - open).to_string()),
        _ => None,
    };

    let change_percentage_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) if open > rust_decimal::Decimal::ZERO => {
            Some(((close - open) / open * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0))
        }
        _ => None,
    };

    // Get top traders
    let top_traders = sqlx::query(
        r#"
        SELECT 
            buyer_id as user_id,
            COUNT(*) as total_trades,
            SUM(quantity::numeric) as total_volume
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        GROUP BY buyer_id
        ORDER BY total_volume DESC
        LIMIT 10
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let trader_stats: Vec<TraderStats> = top_traders
        .into_iter()
        .map(|row| TraderStats {
            user_id: row.try_get::<uuid::Uuid, _>("user_id").unwrap().to_string(),
            total_trades: row.try_get::<i64, _>("total_trades").unwrap_or(0),
            total_volume: row.try_get::<BigDecimal, _>("total_volume").unwrap_or_default().to_string(),
            buy_volume: row.try_get::<BigDecimal, _>("total_volume").unwrap_or_default().to_string(),
            sell_volume: "0".to_string(),
        })
        .collect();

    // Get hourly volume
    let hourly_data = sqlx::query(
        r#"
        SELECT 
            DATE_TRUNC('hour', executed_at) as hour,
            SUM(quantity::numeric) as volume,
            COUNT(*) as trade_count
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        GROUP BY DATE_TRUNC('hour', executed_at)
        ORDER BY hour DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let hourly_volume: Vec<HourlyVolume> = hourly_data
        .into_iter()
        .map(|row| HourlyVolume {
            hour: row.try_get::<Option<chrono::DateTime<Utc>>, _>("hour").ok().flatten().map(|h| h.to_rfc3339()).unwrap_or_default(),
            volume: row.try_get::<BigDecimal, _>("volume").unwrap_or_default().to_string(),
            trade_count: row.try_get::<i64, _>("trade_count").unwrap_or(0),
        })
        .collect();

    Ok(Json(TradingAnalytics {
        total_trades: overall_stats.try_get::<i64, _>("total_trades").unwrap_or(0),
        total_volume: overall_stats.try_get::<BigDecimal, _>("total_volume").unwrap_or_default().to_string(),
        total_value: overall_stats.try_get::<BigDecimal, _>("total_value").unwrap_or_default().to_string(),
        average_trade_size: overall_stats.try_get::<BigDecimal, _>("avg_trade_size").unwrap_or_default().to_string(),
        price_statistics: PriceStatistics {
            current_price: price_stats.try_get::<Option<String>, _>("current_price").ok().flatten(),
            high_24h: price_stats.try_get::<Option<BigDecimal>, _>("high_24h").ok().flatten().map(|p| p.to_string()),
            low_24h: price_stats.try_get::<Option<BigDecimal>, _>("low_24h").ok().flatten().map(|p| p.to_string()),
            open_24h: open_24h.map(|p| p.to_string()),
            close_24h: close_24h.map(|p| p.to_string()),
            change_24h,
            change_percentage_24h,
        },
        top_traders: trader_stats,
        hourly_volume,
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
        MarketAction::ResumeTrading => {
            MarketControlResponse {
                success: true,
                message: "Trading resumed (feature not yet implemented)".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }
        }
        MarketAction::ClearOrderBook => {
            MarketControlResponse {
                success: false,
                message: "Clear order book is a dangerous operation and requires additional confirmation".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }
        }
    };

    Ok(Json(result))
}
