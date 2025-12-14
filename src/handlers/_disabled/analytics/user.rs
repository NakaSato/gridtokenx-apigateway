use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::Result;
use crate::AppState;

use super::types::*;

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
        total_energy_sold_kwh: decimal_to_f64(row.get("total_sold")),
        total_revenue_usd: decimal_to_f64(row.get("total_revenue")),
        average_price_per_kwh: decimal_to_f64(row.get("avg_price")),
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
        total_energy_purchased_kwh: decimal_to_f64(row.get("total_purchased")),
        total_spent_usd: decimal_to_f64(row.get("total_spent")),
        average_price_per_kwh: decimal_to_f64(row.get("avg_price")),
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
        total_volume_kwh: decimal_to_f64(row.get("total_volume")),
        net_revenue_usd: decimal_to_f64(row.get("net_revenue")),
        favorite_energy_source: row.get("favorite_source"),
    })
}
