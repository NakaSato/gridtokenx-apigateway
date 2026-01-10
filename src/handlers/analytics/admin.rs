use axum::{extract::{State, Query}, Json};
use sqlx::Row;
use serde::Serialize;
use utoipa::ToSchema;
use tracing::info;
use chrono::Utc;
use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::Result;
use super::types::*;
use crate::services::audit_logger::AuditEventRecord;
use crate::services::health_check::DetailedHealthStatus;

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminStatsResponse {
    pub total_users: i64,
    pub total_meters: i64,
    pub active_meters: i64,
    pub total_volume_kwh: f64,
    pub total_orders: i64,
    pub settlement_success_rate: f64,
}

/// Get global platform statistics (Admin only)
#[utoipa::path(
    get,
    path = "/api/v1/analytics/admin/stats",
    responses(
        (status = 200, description = "Admin statistics retrieved", body = AdminStatsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin only")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_admin_stats(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<AdminStatsResponse>> {
    info!("📊 Admin: Fetching global platform stats");

    // 1. Total Users
    let total_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    // 2. Meters Stats
    let meter_stats = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE is_verified = true) FROM meters"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0));

    // 3. Trade Stats
    let total_volume = sqlx::query_scalar::<_, Option<rust_decimal::Decimal>>(
        "SELECT SUM(filled_amount) FROM trading_orders WHERE status = 'filled' OR status = 'settled'"
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or_default();

    let total_orders = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM trading_orders")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    // 4. Settlement Success Rate (stub for now, can be expanded with settlement_logs)
    let settlement_success_rate = 100.0; // Assume perfect for now until we have more logs

    Ok(Json(AdminStatsResponse {
        total_users,
        total_meters: meter_stats.0,
        active_meters: meter_stats.1,
        total_volume_kwh: total_volume.to_string().parse().unwrap_or(0.0),
        total_orders,
        settlement_success_rate,
    }))
}

/// Get latest platform activity (Admin only)
#[utoipa::path(
    get,
    path = "/api/v1/analytics/admin/activity",
    responses(
        (status = 200, description = "Admin activity logs retrieved", body = Vec<AuditEventRecord>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin only")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_admin_activity(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<crate::services::audit_logger::AuditEventRecord>>> {
    info!("📊 Admin: Fetching platform activity logs");
    
    let activities = state.audit_logger.get_all_activities(50).await
        .map_err(|e| crate::error::ApiError::Database(e))?;
        
    Ok(Json(activities))
}

/// Get detailed system health (Admin only)
#[utoipa::path(
    get,
    path = "/api/v1/analytics/admin/health",
    responses(
        (status = 200, description = "Detailed system health retrieved", body = DetailedHealthStatus),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin only")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_system_health(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<crate::services::health_check::DetailedHealthStatus>> {
    info!("📊 Admin: Fetching detailed system health");
    
    // Check if we have cached health or perform a new check
    let health = if let Some(cached) = state.health_checker.get_cached_health().await {
        cached
    } else {
        state.health_checker.perform_health_check().await
    };
    
    Ok(Json(health))
}

/// Get economic insights broken down by zones (Admin only)
#[utoipa::path(
    get,
    path = "/api/v1/analytics/admin/zones/economic",
    params(AnalyticsTimeframe),
    responses(
        (status = 200, description = "Zone economic insights retrieved", body = ZoneEconomicInsights),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin only")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_zone_economic_insights(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(params): Query<AnalyticsTimeframe>,
) -> Result<Json<ZoneEconomicInsights>> {
    info!("📊 Admin: Fetching zone economic insights for timeframe: {}", params.timeframe);

    let duration = parse_timeframe(&params.timeframe)?;
    let start_time = Utc::now() - duration;

    // 1. Cross-Zone Trade Stats
    let trade_row = sqlx::query(
        r#"
        SELECT 
            COALESCE(SUM(energy_amount), 0) as total_vol,
            COALESCE(SUM(CASE WHEN buyer_zone_id = seller_zone_id THEN energy_amount ELSE 0 END), 0) as intra_vol,
            COALESCE(SUM(CASE WHEN buyer_zone_id != seller_zone_id THEN energy_amount ELSE 0 END), 0) as inter_vol
        FROM settlements
        WHERE processed_at >= $1 AND status = 'completed'
        "#
    )
    .bind(start_time)
    .fetch_one(&state.db)
    .await?;

    let total_vol = decimal_to_f64(trade_row.get("total_vol"));
    let intra_vol = decimal_to_f64(trade_row.get("intra_vol"));
    let inter_vol = decimal_to_f64(trade_row.get("inter_vol"));

    let trade_stats = ZoneTradeStats {
        timeframe: params.timeframe.clone(),
        total_volume_kwh: total_vol,
        intra_zone_volume_kwh: intra_vol,
        inter_zone_volume_kwh: inter_vol,
        intra_zone_percent: if total_vol > 0.0 { (intra_vol / total_vol) * 100.0 } else { 0.0 },
        inter_zone_percent: if total_vol > 0.0 { (inter_vol / total_vol) * 100.0 } else { 0.0 },
    };

    // 2. Zone Revenue Breakdown
    let revenue_rows = sqlx::query(
        r#"
        SELECT 
            buyer_zone_id as zone_id,
            SUM(total_amount) as total_val,
            SUM(fee_amount) as total_fees,
            SUM(wheeling_charge) as total_wheeling,
            AVG(price_per_kwh) as avg_price
        FROM settlements
        WHERE processed_at >= $1 AND status = 'completed' AND buyer_zone_id IS NOT NULL
        GROUP BY buyer_zone_id
        ORDER BY buyer_zone_id
        "#
    )
    .bind(start_time)
    .fetch_all(&state.db)
    .await?;

    let revenue_breakdown = revenue_rows.iter().map(|row| {
        ZoneRevenueBreakdown {
            zone_id: row.get::<i32, _>("zone_id"),
            total_transaction_value: decimal_to_f64(row.get("total_val")),
            total_platform_fees: decimal_to_f64(row.get("total_fees")),
            total_wheeling_charges: decimal_to_f64(row.get("total_wheeling")),
            avg_price_per_kwh: decimal_to_f64(row.get("avg_price")),
        }
    }).collect();

    Ok(Json(ZoneEconomicInsights {
        timeframe: params.timeframe,
        trade_stats,
        revenue_breakdown,
    }))
}
