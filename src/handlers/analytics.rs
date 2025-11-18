use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

// ==================== REQUEST/RESPONSE TYPES ====================

#[derive(Debug, Deserialize, IntoParams)]
pub struct AnalyticsTimeframe {
    /// Timeframe: 1h, 24h, 7d, 30d (default: 24h)
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    /// Energy source filter (optional)
    pub energy_source: Option<String>,
}

fn default_timeframe() -> String {
    "24h".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketAnalytics {
    pub timeframe: String,
    pub market_overview: MarketOverview,
    pub trading_volume: TradingVolume,
    pub price_statistics: PriceStatistics,
    pub energy_source_breakdown: Vec<EnergySourceStats>,
    pub top_traders: Vec<TraderStats>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketOverview {
    pub total_active_offers: i64,
    pub total_pending_orders: i64,
    pub total_completed_transactions: i64,
    pub total_users_trading: i64,
    pub average_match_time_seconds: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TradingVolume {
    pub total_energy_traded_kwh: f64,
    pub total_value_usd: f64,
    pub number_of_transactions: i64,
    pub average_transaction_size_kwh: f64,
    pub volume_trend_percent: f64, // Compared to previous period
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PriceStatistics {
    pub current_avg_price_per_kwh: f64,
    pub lowest_price_per_kwh: f64,
    pub highest_price_per_kwh: f64,
    pub median_price_per_kwh: f64,
    pub price_volatility_percent: f64,
    pub price_trend_percent: f64, // Compared to previous period
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnergySourceStats {
    pub energy_source: String,
    pub total_volume_kwh: f64,
    pub average_price_per_kwh: f64,
    pub transaction_count: i64,
    pub market_share_percent: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TraderStats {
    pub user_id: String,
    pub username: String,
    pub total_volume_kwh: f64,
    pub transaction_count: i64,
    pub average_price_per_kwh: f64,
    pub role: String, // "user", "admin"
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserTradingStats {
    pub user_id: String,
    pub username: String,
    pub timeframe: String,
    pub as_seller: SellerStats,
    pub as_buyer: BuyerStats,
    pub overall: OverallUserStats,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SellerStats {
    pub offers_created: i64,
    pub offers_fulfilled: i64,
    pub total_energy_sold_kwh: f64,
    pub total_revenue_usd: f64,
    pub average_price_per_kwh: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BuyerStats {
    pub orders_created: i64,
    pub orders_fulfilled: i64,
    pub total_energy_purchased_kwh: f64,
    pub total_spent_usd: f64,
    pub average_price_per_kwh: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OverallUserStats {
    pub total_transactions: i64,
    pub total_volume_kwh: f64,
    pub net_revenue_usd: f64, // revenue - spending
    pub favorite_energy_source: Option<String>,
}

// ==================== HANDLERS ====================

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

/// Get user trading statistics
#[utoipa::path(
    get,
    path = "/api/analytics/my-stats",
    params(AnalyticsTimeframe),
    responses(
        (status = 200, description = "User trading statistics retrieved", body = UserTradingStats),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_user_trading_stats(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Query(params): Query<AnalyticsTimeframe>,
) -> Result<Json<UserTradingStats>> {
    let duration = parse_timeframe(&params.timeframe)?;
    let start_time = Utc::now() - duration;

    // Get seller stats
    let as_seller = get_seller_stats(&state, user.0.sub, start_time).await?;

    // Get buyer stats
    let as_buyer = get_buyer_stats(&state, user.0.sub, start_time).await?;

    // Get overall stats
    let overall = get_overall_user_stats(&state, user.0.sub, start_time).await?;

    Ok(Json(UserTradingStats {
        user_id: user.0.sub.to_string(),
        username: user.0.username.clone(),
        timeframe: params.timeframe,
        as_seller,
        as_buyer,
        overall,
    }))
}

// ==================== HELPER FUNCTIONS ====================

fn parse_timeframe(timeframe: &str) -> Result<Duration> {
    match timeframe {
        "1h" => Ok(Duration::hours(1)),
        "24h" | "1d" => Ok(Duration::hours(24)),
        "7d" => Ok(Duration::days(7)),
        "30d" => Ok(Duration::days(30)),
        _ => Err(ApiError::validation_field(
            "timeframe",
            "Invalid timeframe. Use: 1h, 24h, 7d, or 30d",
        )),
    }
}

fn bigdecimal_to_f64(bd: sqlx::types::BigDecimal) -> f64 {
    bd.to_string().parse().unwrap_or(0.0)
}

async fn get_market_overview(state: &AppState, start_time: DateTime<Utc>) -> Result<MarketOverview> {
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
        "#,
    )
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

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

    let current_energy = bigdecimal_to_f64(current.get("total_energy"));
    let current_value = bigdecimal_to_f64(current.get("total_value"));
    let transaction_count: i64 = current.get("transaction_count");
    let previous_energy = bigdecimal_to_f64(previous.get("total_energy"));

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

    let current_avg = bigdecimal_to_f64(current.get("avg_price"));
    let min_price = bigdecimal_to_f64(current.get("min_price"));
    let max_price = bigdecimal_to_f64(current.get("max_price"));
    let stddev = bigdecimal_to_f64(current.get("stddev_price"));
    let median = bigdecimal_to_f64(current.get("median_price"));
    let previous_avg = bigdecimal_to_f64(previous.get("avg_price"));

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
            total_volume_kwh: bigdecimal_to_f64(row.get("total_volume")),
            average_price_per_kwh: bigdecimal_to_f64(row.get("avg_price")),
            transaction_count: row.get("transaction_count"),
            market_share_percent: bigdecimal_to_f64(row.get("market_share")),
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
            total_volume_kwh: bigdecimal_to_f64(row.get("total_volume")),
            transaction_count: row.get("transaction_count"),
            average_price_per_kwh: bigdecimal_to_f64(row.get("avg_price")),
            role: row.get("role"),
        })
        .collect())
}

async fn get_seller_stats(
    state: &AppState,
    user_id: Uuid,
    start_time: DateTime<Utc>,
) -> Result<SellerStats> {
    let row = sqlx::query(
        r#"
        SELECT 
            COUNT(DISTINCT eo.id) as offers_created,
            COUNT(DISTINCT CASE WHEN eo.status = 'Fulfilled' THEN eo.id END) as offers_fulfilled,
            COALESCE(SUM(et.energy_amount), 0) as total_sold,
            COALESCE(SUM(et.energy_amount * et.price_per_kwh), 0) as total_revenue,
            COALESCE(AVG(et.price_per_kwh), 0) as avg_price
        FROM energy_offers eo
        LEFT JOIN energy_transactions et ON et.offer_id = eo.id AND et.created_at >= $2
        WHERE eo.seller_id = $1 AND eo.created_at >= $2
        "#,
    )
    .bind(user_id)
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    Ok(SellerStats {
        offers_created: row.get("offers_created"),
        offers_fulfilled: row.get("offers_fulfilled"),
        total_energy_sold_kwh: bigdecimal_to_f64(row.get("total_sold")),
        total_revenue_usd: bigdecimal_to_f64(row.get("total_revenue")),
        average_price_per_kwh: bigdecimal_to_f64(row.get("avg_price")),
    })
}

async fn get_buyer_stats(
    state: &AppState,
    user_id: Uuid,
    start_time: DateTime<Utc>,
) -> Result<BuyerStats> {
    let row = sqlx::query(
        r#"
        SELECT 
            COUNT(DISTINCT eo.id) as orders_created,
            COUNT(DISTINCT CASE WHEN eo.status = 'Fulfilled' THEN eo.id END) as orders_fulfilled,
            COALESCE(SUM(et.energy_amount), 0) as total_purchased,
            COALESCE(SUM(et.energy_amount * et.price_per_kwh), 0) as total_spent,
            COALESCE(AVG(et.price_per_kwh), 0) as avg_price
        FROM energy_orders eo
        LEFT JOIN energy_transactions et ON et.order_id = eo.id AND et.created_at >= $2
        WHERE eo.buyer_id = $1 AND eo.created_at >= $2
        "#,
    )
    .bind(user_id)
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    Ok(BuyerStats {
        orders_created: row.get("orders_created"),
        orders_fulfilled: row.get("orders_fulfilled"),
        total_energy_purchased_kwh: bigdecimal_to_f64(row.get("total_purchased")),
        total_spent_usd: bigdecimal_to_f64(row.get("total_spent")),
        average_price_per_kwh: bigdecimal_to_f64(row.get("avg_price")),
    })
}

async fn get_overall_user_stats(
    state: &AppState,
    user_id: Uuid,
    start_time: DateTime<Utc>,
) -> Result<OverallUserStats> {
    let row = sqlx::query(
        r#"
        WITH user_transactions AS (
            SELECT 
                et.*,
                eo.energy_source,
                CASE WHEN et.seller_id = $1 THEN 'sell' ELSE 'buy' END as trade_type
            FROM energy_transactions et
            LEFT JOIN energy_offers eo ON et.offer_id = eo.id
            WHERE (et.seller_id = $1 OR et.buyer_id = $1) 
            AND et.created_at >= $2
            AND et.status = 'Completed'
        ),
        revenue_calc AS (
            SELECT 
                SUM(CASE WHEN trade_type = 'sell' THEN energy_amount * price_per_kwh ELSE 0 END) as revenue,
                SUM(CASE WHEN trade_type = 'buy' THEN energy_amount * price_per_kwh ELSE 0 END) as spending
            FROM user_transactions
        ),
        source_ranking AS (
            SELECT energy_source, COUNT(*) as count
            FROM user_transactions
            WHERE energy_source IS NOT NULL
            GROUP BY energy_source
            ORDER BY count DESC
            LIMIT 1
        )
        SELECT 
            COUNT(*) as total_transactions,
            COALESCE(SUM(energy_amount), 0) as total_volume,
            COALESCE((SELECT revenue - spending FROM revenue_calc), 0) as net_revenue,
            (SELECT energy_source FROM source_ranking) as favorite_source
        FROM user_transactions
        "#,
    )
    .bind(user_id)
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    Ok(OverallUserStats {
        total_transactions: row.get("total_transactions"),
        total_volume_kwh: bigdecimal_to_f64(row.get("total_volume")),
        net_revenue_usd: bigdecimal_to_f64(row.get("net_revenue")),
        favorite_energy_source: row.get("favorite_source"),
    })
}
