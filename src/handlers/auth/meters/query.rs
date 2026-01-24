use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use tracing::{info, error};
use uuid::Uuid;
use crate::auth::middleware::AuthenticatedUser;
use crate::AppState;
use super::super::types::{
    MeterResponse, MeterFilterParams, ReadingFilterParams, MeterReadingResponse, MeterStats,
};

/// Get user's registered meters from database
#[utoipa::path(
    get,
    path = "/api/v1/meters",
    responses(
        (status = 200, description = "List of user meters", body = Vec<MeterResponse>),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "meters"
)]
pub async fn get_my_meters(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<Vec<MeterResponse>> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    info!("📊 Get meters request");

    if let Ok(claims) = state.jwt_service.decode_token(token) {
        // Query meters from database including coordinates
        let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>, Option<i32>)>(
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude, m.zone_id
             FROM meters m
             JOIN users u ON m.user_id = u.id
             WHERE m.user_id = $1"
        )
        .bind(claims.sub)
        .fetch_all(&state.db)
        .await;

        if let Ok(meters) = meters_result {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng, zone)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
                    zone_id: *zone,
                }
            }).collect();
            
            info!("✅ Returning {} meters from database", responses.len());
            return Json(responses);
        }
    }

    Json(vec![])
}

/// Get all registered meters (for simulator)
#[utoipa::path(
    get,
    path = "/api/v1/meters/all",
    responses(
        (status = 200, description = "All registered meters", body = Vec<MeterResponse>),
    ),
    tag = "meters"
)]
pub async fn get_registered_meters(
    State(state): State<AppState>,
) -> Json<Vec<MeterResponse>> {
    info!("📊 Get all registered meters");
    
    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>, Option<i32>)>(
        "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude, m.zone_id
         FROM meters m
         JOIN users u ON m.user_id = u.id
         WHERE m.is_verified = true"
    )
    .fetch_all(&state.db)
    .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng, zone)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
                    zone_id: *zone,
                }
            }).collect();
            
            info!("✅ Returning {} registered meters from database", responses.len());
            Json(responses)
        }
        Err(e) => {
            info!("⚠️ Database error: {}", e);
            Json(vec![])
        }
    }
}

/// Get meters with optional status filter
#[utoipa::path(
    get,
    path = "/api/v1/meters/filter",
    params(MeterFilterParams),
    responses(
        (status = 200, description = "Filtered meters", body = Vec<MeterResponse>),
    ),
    tag = "meters"
)]
pub async fn get_registered_meters_filtered(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MeterFilterParams>,
) -> Json<Vec<MeterResponse>> {
    info!("📊 Get meters with filter: {:?}", params.status);
    
    let query = match params.status.as_deref() {
        Some("verified") | Some("active") => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude, m.zone_id
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = true"
        }
        Some("pending") => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude, m.zone_id
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = false"
        }
        _ => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude, m.zone_id
             FROM meters m JOIN users u ON m.user_id = u.id"
        }
    };

    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>, Option<i32>)>(query)
        .fetch_all(&state.db)
        .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng, zone)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
                    zone_id: *zone,
                }
            }).collect();
            Json(responses)
        }
        Err(e) => {
            info!("⚠️ Database error: {}", e);
            Json(vec![])
        }
    }
}

/// Get meter readings for the authenticated user
#[utoipa::path(
    get,
    path = "/api/v1/meters/readings",
    params(ReadingFilterParams),
    responses(
        (status = 200, description = "List of readings", body = Vec<MeterReadingResponse>),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "meters"
)]
pub async fn get_my_readings(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<ReadingFilterParams>,
) -> Json<Vec<MeterReadingResponse>> {
    info!("📊 Get readings request for user {}", claims.sub);

    let limit = params.limit.unwrap_or(50).min(1000);
    let offset = params.offset.unwrap_or(0);
        
        // We query meter_readings. Note: Partition key is reading_timestamp.
        // We order by reading_timestamp DESC.
        let readings_result = sqlx::query_as::<_, MeterReadingResponse>(
            "SELECT 
                id, 
                meter_serial, 
                kwh_amount::FLOAT8 as kwh, 
                reading_timestamp as timestamp, 
                created_at as submitted_at, 
                minted, 
                mint_tx_signature as tx_signature,
                NULL::text as message
             FROM meter_readings
             WHERE user_id = $1
             ORDER BY reading_timestamp DESC
             LIMIT $2 OFFSET $3"
        )
        .bind(claims.sub)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await;

    match readings_result {
        Ok(readings) => {
            info!("✅ Returning {} readings", readings.len());
            return Json(readings);
        }
        Err(e) => {
            info!("⚠️ Error fetching readings: {}", e);
        }
    }
    
    Json(vec![])
}

/// Get aggregated meter stats for the user
#[utoipa::path(
    get,
    path = "/api/v1/meters/stats",
    responses(
        (status = 200, description = "Meter stats", body = MeterStats),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "meters"
)]
pub async fn get_meter_stats(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> Json<MeterStats> {
    // Query aggregated stats
    let stats_result = sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<chrono::DateTime<chrono::Utc>>, Option<f64>, Option<i64>, Option<f64>, Option<i64>)>(
            "SELECT 
                SUM(energy_generated)::FLOAT8 as total_produced,
                SUM(energy_consumed)::FLOAT8 as total_consumed,
                MAX(reading_timestamp) as last_reading_time,
                SUM(CASE WHEN minted = true THEN kwh_amount ELSE 0 END)::FLOAT8 as total_minted,
                COUNT(CASE WHEN minted = true THEN 1 END) as minted_count,
                SUM(CASE WHEN minted = false AND kwh_amount > 0 THEN kwh_amount ELSE 0 END)::FLOAT8 as pending_mint,
                COUNT(CASE WHEN minted = false AND kwh_amount > 0 THEN 1 END) as pending_mint_count
             FROM meter_readings
             WHERE user_id = $1"
        )
        .bind(claims.sub)
        .fetch_one(&state.db)
        .await;

    match stats_result {
        Ok((produced, consumed, last_time, minted, m_count, pending, p_count)) => {
            let stats = MeterStats {
                total_produced: produced.unwrap_or(0.0),
                total_consumed: consumed.unwrap_or(0.0),
                last_reading_time: last_time,
                total_minted: minted.unwrap_or(0.0),
                total_minted_count: m_count.unwrap_or(0),
                pending_mint: pending.unwrap_or(0.0),
                pending_mint_count: p_count.unwrap_or(0),
            };
            return Json(stats);
        }
        Err(e) => {
            error!("⚠️ Error fetching meter stats: {}", e);
        }
    }
    
    Json(MeterStats::default())
}
