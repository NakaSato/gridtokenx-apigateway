use axum::{extract::State, Json};
use sqlx::Row;
use serde::Serialize;
use utoipa::ToSchema;
use tracing::info;
use crate::AppState;
use crate::error::Result;
use super::types::decimal_to_f64;

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneTradingStats {
    pub zone_id: i32,
    pub zone_name: String,
    pub active_trades: i64,
    pub total_volume_kwh: f64,
    pub avg_price_per_kwh: f64,
    pub total_trade_value: f64,
    pub prosumer_count: i64,
    pub consumer_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneTradingStatsResponse {
    pub zones: Vec<ZoneTradingStats>,
    pub total_active_trades: i64,
    pub total_volume_kwh: f64,
}

/// Get zone-level trading statistics (public endpoint)
#[utoipa::path(
    get,
    path = "/api/v1/analytics/zones/trading",
    responses(
        (status = 200, description = "Zone trading statistics retrieved", body = ZoneTradingStatsResponse)
    )
)]
pub async fn get_zone_trading_stats(
    State(state): State<AppState>,
) -> Result<Json<ZoneTradingStatsResponse>> {
    info!("📊 Fetching zone trading statistics");

    // Get zone-level stats from meters and orders
    let zone_rows = sqlx::query(
        r#"
        WITH zone_meters AS (
            SELECT 
                zone_id,
                COUNT(*) as total_meters,
                COUNT(*) FILTER (WHERE meter_type = 'prosumer') as prosumer_count,
                COUNT(*) FILTER (WHERE meter_type = 'consumer') as consumer_count
            FROM meters
            WHERE zone_id IS NOT NULL
            GROUP BY zone_id
        ),
        zone_trades AS (
            SELECT 
                COALESCE(m.zone_id, 0) as zone_id,
                COUNT(*) FILTER (WHERE o.status = 'open' OR o.status = 'partially_filled') as active_trades,
                COALESCE(SUM(o.filled_amount), 0) as total_volume,
                COALESCE(AVG(o.price), 0) as avg_price,
                COALESCE(SUM(o.filled_amount * o.price), 0) as total_value
            FROM trading_orders o
            LEFT JOIN meters m ON o.meter_id = m.id
            WHERE o.created_at >= NOW() - INTERVAL '24 hours'
            GROUP BY m.zone_id
        )
        SELECT 
            COALESCE(zm.zone_id, zt.zone_id, 0) as zone_id,
            COALESCE(zt.active_trades, 0) as active_trades,
            COALESCE(zt.total_volume, 0) as total_volume,
            COALESCE(zt.avg_price, 0) as avg_price,
            COALESCE(zt.total_value, 0) as total_value,
            COALESCE(zm.prosumer_count, 0) as prosumer_count,
            COALESCE(zm.consumer_count, 0) as consumer_count
        FROM zone_meters zm
        FULL OUTER JOIN zone_trades zt ON zm.zone_id = zt.zone_id
        ORDER BY zone_id
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let zones: Vec<ZoneTradingStats> = zone_rows.iter().map(|row| {
        let zone_id: i32 = row.get("zone_id");
        ZoneTradingStats {
            zone_id,
            zone_name: format!("Zone {}", zone_id),
            active_trades: row.get::<i64, _>("active_trades"),
            total_volume_kwh: decimal_to_f64(row.get("total_volume")),
            avg_price_per_kwh: decimal_to_f64(row.get("avg_price")),
            total_trade_value: decimal_to_f64(row.get("total_value")),
            prosumer_count: row.get::<i64, _>("prosumer_count"),
            consumer_count: row.get::<i64, _>("consumer_count"),
        }
    }).collect();

    let total_active_trades: i64 = zones.iter().map(|z| z.active_trades).sum();
    let total_volume_kwh: f64 = zones.iter().map(|z| z.total_volume_kwh).sum();

    Ok(Json(ZoneTradingStatsResponse {
        zones,
        total_active_trades,
        total_volume_kwh,
    }))
}
