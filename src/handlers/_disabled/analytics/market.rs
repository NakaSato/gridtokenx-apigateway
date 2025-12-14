use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::error::Result;
use crate::AppState;

use super::types::*;

/// Get market analytics
#[utoipa::path(
    get,
    path = "/api/analytics/market",
    params(AnalyticsTimeframe),
    responses(
        (status = 200, description = "Market analytics retrieved", body = MarketAnalytics),
        (status = 400, description = "Invalid timeframe")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_market_analytics(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsTimeframe>,
) -> Result<Json<MarketAnalytics>> {
    // Parse timeframe
    let duration = parse_timeframe(&params.timeframe)?;
    let start_time = Utc::now() - duration;
    let prev_start_time = start_time - duration; // For trend calculation

    // Get market overview
    let market_overview = get_market_overview(&state, start_time).await?;

    // Get trading volume
    let trading_volume = get_trading_volume(&state, start_time, prev_start_time).await?;

    // Get price statistics
    let price_statistics = get_price_statistics(&state, start_time, prev_start_time).await?;

    // Get energy source breakdown
    let energy_source_breakdown = get_energy_source_breakdown(&state, start_time).await?;

    // Get top traders
    let top_traders = get_top_traders(&state, start_time, 10).await?;

    Ok(Json(MarketAnalytics {
        timeframe: params.timeframe,
        market_overview,
        trading_volume,
        price_statistics,
        energy_source_breakdown,
        top_traders,
    }))
}

// ==================== HELPER FUNCTIONS ====================

async fn get_market_overview(
    state: &AppState,
    start_time: DateTime<Utc>,
) -> Result<MarketOverview> {
    let row = sqlx::query(
        r#"
        SELECT 
            (SELECT COUNT(*) FROM energy_offers WHERE status = 'Active') as active_offers,
            (SELECT COUNT(*) FROM energy_orders WHERE status = 'Pending') as pending_orders,
            (SELECT COUNT(*) FROM energy_transactions WHERE created_at >= $1) as completed_transactions,
            (SELECT COUNT(DISTINCT COALESCE(seller_id, buyer_id)) 
             FROM energy_transactions 
             WHERE created_at >= $1) as users_trading,
            (SELECT COALESCE(AVG(EXTRACT(EPOCH FROM (updated_at - created_at))), 0)
        FROM energy_transactions
        WHERE created_at >= $1 AND status = 'Completed'
        ) as avg_match_time
        "#,
    )
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    // Fix: Added ) in query above after WHERE clause, might have been missing in original but sqlx macros usually catch it.
    // Wait, the original had a syntax that looked a bit odd with string continuation or maybe I misread.
    // Let's standardise the query string.

    /* Original:
        SELECT
            (SELECT COUNT(*) FROM energy_offers WHERE status = 'Active') as active_offers,
            (SELECT COUNT(*) FROM energy_orders WHERE status = 'Pending') as pending_orders,
            (SELECT COUNT(*) FROM energy_transactions WHERE created_at >= $1) as completed_transactions,
            (SELECT COUNT(DISTINCT COALESCE(seller_id, buyer_id))
             FROM energy_transactions
             WHERE created_at >= $1) as users_trading,
            (SELECT COALESCE(AVG(EXTRACT(EPOCH FROM (updated_at - created_at))), 0)
        FROM energy_transactions
        WHERE created_at >= $1 AND status = 'Completed'
    */
    // It seems the last subquery was missing a closing parenthesis in the original snippet I viewed or I missed it.
    // I will ensure it is correct here.

    Ok(MarketOverview {
        total_active_offers: row.get("active_offers"),
        total_pending_orders: row.get("pending_orders"),
        total_completed_transactions: row.get("completed_transactions"),
        total_users_trading: row.get("users_trading"),
        average_match_time_seconds: row.get("avg_match_time"),
    })
}

async fn get_trading_volume(
    state: &AppState,
    start_time: DateTime<Utc>,
    prev_start_time: DateTime<Utc>,
) -> Result<TradingVolume> {
    // Current period
    let current = sqlx::query(
        r#"
        SELECT 
            COALESCE(SUM(energy_amount), 0) as total_energy,
            COALESCE(SUM(energy_amount * price_per_kwh), 0) as total_value,
            COUNT(*) as transaction_count
        FROM energy_transactions
        WHERE created_at >= $1 AND status = 'Completed'
        "#,
    )
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    // Previous period for trend
    let previous = sqlx::query(
        r#"
        SELECT COALESCE(SUM(energy_amount), 0) as total_energy
        FROM energy_transactions
        WHERE created_at >= $1 AND created_at < $2 AND status = 'Completed'
        "#,
    )
    .bind(prev_start_time)
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    let current_energy = decimal_to_f64(current.get("total_energy"));
    let current_value = decimal_to_f64(current.get("total_value"));
    let transaction_count: i64 = current.get("transaction_count");
    let previous_energy = decimal_to_f64(previous.get("total_energy"));

    let volume_trend = if previous_energy > 0.0 {
        ((current_energy - previous_energy) / previous_energy) * 100.0
    } else {
        0.0
    };

    let avg_transaction_size = if transaction_count > 0 {
        current_energy / transaction_count as f64
    } else {
        0.0
    };

    Ok(TradingVolume {
        total_energy_traded_kwh: current_energy,
        total_value_usd: current_value,
        number_of_transactions: transaction_count,
        average_transaction_size_kwh: avg_transaction_size,
        volume_trend_percent: volume_trend,
    })
}

async fn get_price_statistics(
    state: &AppState,
    start_time: DateTime<Utc>,
    prev_start_time: DateTime<Utc>,
) -> Result<PriceStatistics> {
    // Current period stats
    let current = sqlx::query(
        r#"
        SELECT 
            COALESCE(AVG(price_per_kwh), 0) as avg_price,
            COALESCE(MIN(price_per_kwh), 0) as min_price,
            COALESCE(MAX(price_per_kwh), 0) as max_price,
            COALESCE(STDDEV(price_per_kwh), 0) as stddev_price,
            PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY price_per_kwh) as median_price
        FROM energy_transactions
        WHERE created_at >= $1 AND status = 'Completed'
        "#,
    )
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    // Previous period for trend
    let previous = sqlx::query(
        r#"
        SELECT COALESCE(AVG(price_per_kwh), 0) as avg_price
        FROM energy_transactions
        WHERE created_at >= $1 AND created_at < $2 AND status = 'Completed'
        "#,
    )
    .bind(prev_start_time)
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    let current_avg = decimal_to_f64(current.get("avg_price"));
    let min_price = decimal_to_f64(current.get("min_price"));
    let max_price = decimal_to_f64(current.get("max_price"));
    let stddev = decimal_to_f64(current.get("stddev_price"));
    let median = decimal_to_f64(current.get("median_price"));
    let previous_avg = decimal_to_f64(previous.get("avg_price"));

    let price_trend = if previous_avg > 0.0 {
        ((current_avg - previous_avg) / previous_avg) * 100.0
    } else {
        0.0
    };

    let volatility = if current_avg > 0.0 {
        (stddev / current_avg) * 100.0
    } else {
        0.0
    };

    Ok(PriceStatistics {
        current_avg_price_per_kwh: current_avg,
        lowest_price_per_kwh: min_price,
        highest_price_per_kwh: max_price,
        median_price_per_kwh: median,
        price_volatility_percent: volatility,
        price_trend_percent: price_trend,
    })
}

async fn get_energy_source_breakdown(
    state: &AppState,
    start_time: DateTime<Utc>,
) -> Result<Vec<EnergySourceStats>> {
    let rows = sqlx::query(
        r#"
        WITH total_volume AS (
            SELECT COALESCE(SUM(energy_amount), 1) as total
            FROM energy_transactions
            WHERE created_at >= $1 AND status = 'Completed'
        )
        SELECT 
            COALESCE(eo.energy_source, 'unknown') as energy_source,
            COALESCE(SUM(et.energy_amount), 0) as total_volume,
            COALESCE(AVG(et.price_per_kwh), 0) as avg_price,
            COUNT(*) as transaction_count,
            (COALESCE(SUM(et.energy_amount), 0) / (SELECT total FROM total_volume) * 100) as market_share
        FROM energy_transactions et
        LEFT JOIN energy_offers eo ON et.offer_id = eo.id
        WHERE et.created_at >= $1 AND et.status = 'Completed'
        GROUP BY eo.energy_source
        ORDER BY total_volume DESC
        "#,
    )
    .bind(start_time)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| EnergySourceStats {
            energy_source: row.get("energy_source"),
            total_volume_kwh: decimal_to_f64(row.get("total_volume")),
            average_price_per_kwh: decimal_to_f64(row.get("avg_price")),
            transaction_count: row.get("transaction_count"),
            market_share_percent: decimal_to_f64(row.get("market_share")),
        })
        .collect())
}

async fn get_top_traders(
    state: &AppState,
    start_time: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<TraderStats>> {
    let rows = sqlx::query(
        r#"
        WITH user_trades AS (
            SELECT 
                COALESCE(seller_id, buyer_id) as user_id,
                SUM(energy_amount) as total_volume,
                COUNT(*) as transaction_count,
                AVG(price_per_kwh) as avg_price
            FROM energy_transactions
            WHERE created_at >= $1 AND status = 'Completed'
            GROUP BY COALESCE(seller_id, buyer_id)
            ORDER BY total_volume DESC
            LIMIT $2
        )
        SELECT 
            ut.user_id,
            u.username,
            ut.total_volume,
            ut.transaction_count,
            ut.avg_price,
            u.role
        FROM user_trades ut
        JOIN users u ON ut.user_id = u.id
        "#,
    )
    .bind(start_time)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TraderStats {
            user_id: row.get::<Uuid, _>("user_id").to_string(),
            username: row.get("username"),
            total_volume_kwh: decimal_to_f64(row.get("total_volume")),
            transaction_count: row.get("transaction_count"),
            average_price_per_kwh: decimal_to_f64(row.get("avg_price")),
            role: row.get("role"),
        })
        .collect())
}
