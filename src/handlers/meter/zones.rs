use axum::{
    extract::{State, Path},
    Json,
};
use serde::Serialize;
use tracing::info;
use crate::AppState;
use crate::error::{ApiError, Result};
use utoipa::ToSchema;

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ZoneSummary {
    pub zone_id: i32,
    pub meter_count: i64,
    pub total_generation: Option<f64>,
    pub total_consumption: Option<f64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneStats {
    pub zone_id: i32,
    pub total_generation: f64,
    pub total_consumption: f64,
    pub net_balance: f64,
    pub active_meters: i64,
}

/// Get summary of all zones
#[utoipa::path(
    get,
    path = "/api/v1/meters/zones",
    responses(
        (status = 200, description = "Summary of all zones", body = Vec<ZoneSummary>)
    ),
    tag = "meters"
)]
pub async fn get_zones(
    State(state): State<AppState>,
) -> Result<Json<Vec<ZoneSummary>>> {
    info!("📊 Getting summary for all zones");

    let zones = sqlx::query_as::<_, ZoneSummary>(
        r#"SELECT 
            m.zone_id,
            COUNT(m.id) as meter_count,
            SUM(lr.latest_gen) as total_generation,
            SUM(lr.latest_cons) as total_consumption
           FROM meters m
           LEFT JOIN LATERAL (
               SELECT 
                energy_generated::FLOAT8 as latest_gen,
                energy_consumed::FLOAT8 as latest_cons
               FROM meter_readings
               WHERE meter_serial = m.serial_number
               ORDER BY reading_timestamp DESC
               LIMIT 1
           ) lr ON true
           WHERE m.zone_id IS NOT NULL
           GROUP BY m.zone_id
           ORDER BY m.zone_id ASC"#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok(Json(zones))
}

/// Get statistical overview of a specific zone
#[utoipa::path(
    get,
    path = "/api/v1/meters/zones/{zone_id}/stats",
    params(
        ("zone_id" = i32, Path, description = "Zone ID")
    ),
    responses(
        (status = 200, description = "Detailed zone statistics", body = ZoneStats),
        (status = 404, description = "Zone not found")
    ),
    tag = "meters"
)]
pub async fn get_zone_stats(
    State(state): State<AppState>,
    Path(zone_id): Path<i32>,
) -> Result<Json<ZoneStats>> {
    info!("📊 Getting statistics for zone {}", zone_id);

    let stats = sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<i64>)>(
        r#"SELECT 
            SUM(lr.latest_gen) as total_gen,
            SUM(lr.latest_cons) as total_cons,
            COUNT(m.id) FILTER (WHERE lr.latest_gen > 0 OR lr.latest_cons > 0) as active_count
           FROM meters m
           LEFT JOIN LATERAL (
               SELECT 
                energy_generated::FLOAT8 as latest_gen,
                energy_consumed::FLOAT8 as latest_cons
               FROM meter_readings
               WHERE meter_serial = m.serial_number
               ORDER BY reading_timestamp DESC
               LIMIT 1
           ) lr ON true
           WHERE m.zone_id = $1"#
    )
    .bind(zone_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let (gen, cons, active) = (stats.0.unwrap_or(0.0), stats.1.unwrap_or(0.0), stats.2.unwrap_or(0));

    Ok(Json(ZoneStats {
        zone_id,
        total_generation: gen,
        total_consumption: cons,
        net_balance: gen - cons,
        active_meters: active,
    }))
}
