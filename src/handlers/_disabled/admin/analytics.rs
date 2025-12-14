use axum::{extract::State, response::Json};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
use std::str::FromStr;
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::AppState;

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
        "#,
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

    let open_24h = price_stats
        .try_get::<Option<Decimal>, _>("open_24h")
        .ok()
        .flatten()
        .and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());
    let close_24h = price_stats
        .try_get::<Option<Decimal>, _>("close_24h")
        .ok()
        .flatten()
        .and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());

    let change_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) => Some((close - open).to_string()),
        _ => None,
    };

    let change_percentage_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) if open > rust_decimal::Decimal::ZERO => Some(
            ((close - open) / open * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
        ),
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
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let trader_stats: Vec<TraderStats> = top_traders
        .into_iter()
        .map(|row| TraderStats {
            user_id: row
                .try_get::<uuid::Uuid, _>("user_id")
                .map(|u| u.to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            total_trades: row.try_get::<i64, _>("total_trades").unwrap_or(0),
            total_volume: row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
            buy_volume: row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
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
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let hourly_volume: Vec<HourlyVolume> = hourly_data
        .into_iter()
        .map(|row| HourlyVolume {
            hour: row
                .try_get::<Option<chrono::DateTime<Utc>>, _>("hour")
                .ok()
                .flatten()
                .map(|h| h.to_rfc3339())
                .unwrap_or_default(),
            volume: row
                .try_get::<Decimal, _>("volume")
                .unwrap_or_default()
                .to_string(),
            trade_count: row.try_get::<i64, _>("trade_count").unwrap_or(0),
        })
        .collect();

    Ok(Json(TradingAnalytics {
        total_trades: overall_stats.try_get::<i64, _>("total_trades").unwrap_or(0),
        total_volume: overall_stats
            .try_get::<Decimal, _>("total_volume")
            .unwrap_or_default()
            .to_string(),
        total_value: overall_stats
            .try_get::<Decimal, _>("total_value")
            .unwrap_or_default()
            .to_string(),
        average_trade_size: overall_stats
            .try_get::<Decimal, _>("avg_trade_size")
            .unwrap_or_default()
            .to_string(),
        price_statistics: PriceStatistics {
            current_price: price_stats
                .try_get::<Option<String>, _>("current_price")
                .ok()
                .flatten(),
            high_24h: price_stats
                .try_get::<Option<Decimal>, _>("high_24h")
                .ok()
                .flatten()
                .map(|p| p.to_string()),
            low_24h: price_stats
                .try_get::<Option<Decimal>, _>("low_24h")
                .ok()
                .flatten()
                .map(|p| p.to_string()),
            open_24h: open_24h.map(|p| p.to_string()),
            close_24h: close_24h.map(|p| p.to_string()),
            change_24h,
            change_percentage_24h,
        },
        top_traders: trader_stats,
        hourly_volume,
    }))
}
