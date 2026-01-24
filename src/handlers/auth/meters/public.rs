use axum::{
    extract::{State, Query},
    Json,
};
use tracing::{info, error};
use crate::AppState;
use super::super::types::{
    PublicMeterResponse, PublicGridStatusResponse, GridHistoryParams,
};

/// Get all verified meters - PUBLIC endpoint (no auth required)
/// 
/// Returns only publicly-safe information for map display.
/// Excludes sensitive data like wallet addresses, serial numbers, and internal IDs.
#[utoipa::path(
    get,
    path = "/api/v1/public/meters",
    responses(
        (status = 200, description = "List of verified meters (public info only)", body = Vec<PublicMeterResponse>)
    ),
    tag = "meters"
)]
pub async fn public_get_meters(
    State(state): State<AppState>,
) -> Json<Vec<PublicMeterResponse>> {
    info!("Public meters request for map display");
    
    // Query public-safe fields with latest reading data including telemetry
    let meters_result = sqlx::query_as::<_, (
        String, String, bool, Option<f64>, Option<f64>, 
        Option<f64>, Option<f64>,
        Option<f64>, Option<f64>, Option<f64>, Option<f64>,
        Option<f64>, Option<f64>, Option<i32>
    )>(
        r#"
        SELECT 
            m.meter_type, 
            m.location, 
            m.is_verified, 
            m.latitude, 
            m.longitude,
            lr.current_generation,
            lr.current_consumption,
            lr.voltage,
            lr.current_amps,
            lr.frequency,
            lr.power_factor,
            lr.surplus_energy,
            lr.deficit_energy,
            m.zone_id
        FROM meters m
        LEFT JOIN LATERAL (
            SELECT 
                energy_generated::FLOAT8 as current_generation, 
                energy_consumed::FLOAT8 as current_consumption,
                voltage::FLOAT8 as voltage,
                current_amps::FLOAT8 as current_amps,
                frequency::FLOAT8 as frequency,
                power_factor::FLOAT8 as power_factor,
                surplus_energy::FLOAT8 as surplus_energy,
                deficit_energy::FLOAT8 as deficit_energy
            FROM meter_readings
            WHERE meter_serial = m.serial_number
            ORDER BY reading_timestamp DESC
            LIMIT 1
        ) lr ON true
        WHERE m.is_verified = true
        "#
    )
    .fetch_all(&state.db)
    .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<PublicMeterResponse> = meters.iter().map(|(
                mtype, loc, verified, lat, lng, gen, cons,
                voltage, current, frequency, power_factor,
                surplus, deficit, zone
            )| {
                PublicMeterResponse {
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    latitude: *lat,
                    longitude: *lng,
                    current_generation: *gen,
                    current_consumption: *cons,
                    voltage: *voltage,
                    current: *current,
                    frequency: *frequency,
                    power_factor: *power_factor,
                    surplus_energy: *surplus,
                    deficit_energy: *deficit,
                    zone_id: *zone,
                }
            }).collect();
            
            info!("✅ Public API: Returning {} meters for map (with telemetry)", responses.len());
            Json(responses)
        }
        Err(e) => {
            error!("❌ Public meters error: {}", e);
            Json(vec![])
        }
    }
}

/// Get aggregate grid status - PUBLIC endpoint (no auth required)
#[utoipa::path(
    get,
    path = "/api/v1/public/grid-status",
    responses(
        (status = 200, description = "Aggregate grid status", body = PublicGridStatusResponse)
    ),
    tag = "meters"
)]
pub async fn public_grid_status(
    State(state): State<AppState>,
) -> Json<PublicGridStatusResponse> {
    info!("Public grid status request (serving from DashboardService cache)");

    let metrics = state.dashboard_service.get_grid_status().await;

    Json(PublicGridStatusResponse {
        total_generation: metrics.total_generation,
        total_consumption: metrics.total_consumption,
        net_balance: metrics.net_balance,
        active_meters: metrics.active_meters,
        co2_saved_kg: metrics.co2_saved_kg,
        timestamp: metrics.timestamp,
        zones: metrics.zones.clone(),
    })
}

/// Get aggregate grid status history - PUBLIC endpoint (no auth required)
#[utoipa::path(
    get,
    path = "/api/v1/public/grid-status/history",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum data points to return")
    ),
    responses(
        (status = 200, description = "Historical aggregate grid status", body = Vec<PublicGridStatusResponse>)
    ),
    tag = "meters"
)]
pub async fn public_grid_history(
    State(state): State<AppState>,
    Query(params): Query<GridHistoryParams>,
) -> Json<Vec<PublicGridStatusResponse>> {
    info!("Public grid status history request (limit: {:?})", params.limit);

    let limit = params.limit.unwrap_or(1440); // Default to last 24 hours if 1 snapshot/min
    
    match state.dashboard_service.get_grid_history(limit as i64).await {
        Ok(history) => {
            let response = history.into_iter().map(|h| PublicGridStatusResponse {
                total_generation: h.total_generation,
                total_consumption: h.total_consumption,
                net_balance: h.net_balance,
                active_meters: h.active_meters,
                co2_saved_kg: h.co2_saved_kg,
                timestamp: h.timestamp,
                zones: h.zones.clone(),
            }).collect();
            Json(response)
        },
        Err(e) => {
            error!("❌ Failed to fetch grid history: {}", e);
            Json(vec![]) // Return empty list on error for now
        }
    }
}
