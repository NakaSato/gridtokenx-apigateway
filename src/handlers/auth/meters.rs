//! Meters Handlers Module
//!
//! Meter management handlers: registration, verification, readings, etc.

use axum::{
    extract::{State, Query},
    http::HeaderMap,
    Json,
};
use tracing::{info, error, warn};
use uuid::Uuid;
use crate::auth::middleware::AuthenticatedUser;
use serde_json;
use crate::services::meter_analyzer::{check_alerts, calculate_health_score};
use rust_decimal::prelude::ToPrimitive;
use tracing::debug;

use crate::AppState;
use super::types::{
    MeterResponse, PublicMeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    VerifyMeterRequest, MeterFilterParams, UpdateMeterStatusRequest,
    CreateReadingRequest, CreateReadingResponse, MeterReadingResponse, ReadingFilterParams,
    CreateReadingParams, MeterStats, PublicGridStatusResponse, GridHistoryParams,
    CreateBatchReadingRequest, BatchReadingResponse,
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

/// Register a new meter to user account
#[utoipa::path(
    post,
    path = "/api/v1/meters",
    request_body = RegisterMeterRequest,
    responses(
        (status = 200, description = "Meter registered", body = RegisterMeterResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Meter already exists")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "meters"
)]
pub async fn register_meter(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(request): Json<RegisterMeterRequest>,
) -> Json<RegisterMeterResponse> {
    info!("📊 Register meter request: {}", request.serial_number);

    let user_id = claims.sub;
    let meter_id = Uuid::new_v4();
    let meter_type = request.meter_type.unwrap_or_else(|| "solar".to_string());
    let location = request.location.unwrap_or_else(|| "Not specified".to_string());

    // Check if meter serial already exists
    let existing = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM meters WHERE serial_number = $1"
    )
    .bind(&request.serial_number)
    .fetch_optional(&state.db)
    .await;

    if let Ok(Some(_)) = existing {
        return Json(RegisterMeterResponse {
            success: false,
            message: format!("Meter {} is already registered to another account", request.serial_number),
            meter: None,
        });
    }

    // Insert meter into database with coordinates and zone
    let insert_result = sqlx::query(
        "INSERT INTO meters (id, user_id, serial_number, meter_type, location, latitude, longitude, zone_id, is_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, NOW(), NOW())"
    )
    .bind(meter_id)
    .bind(user_id)
    .bind(&request.serial_number)
    .bind(&meter_type)
    .bind(&location)
    .bind(request.latitude)
    .bind(request.longitude)
    .bind(request.zone_id)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Meter {} registered for user {} (Zone: {:?})", request.serial_number, user_id, request.zone_id);
            
            // Sync to meter_registry for FK constraints
            let _ = sqlx::query(
                "INSERT INTO meter_registry (id, user_id, meter_serial, meter_type, location_address, meter_key_hash, verification_method, verification_status, zone_id)
                 VALUES ($1, $2, $3, $4, $5, 'mock_hash', 'serial', 'verified', $6)"
            )
            .bind(meter_id)
            .bind(user_id)
            .bind(&request.serial_number)
            .bind(&meter_type)
            .bind(&location)
            .bind(request.zone_id)
            .execute(&state.db)
            .await
            .map_err(|e| error!("Failed to sync meter_registry: {}", e));

            // Get user wallet for response
            let wallet = sqlx::query_as::<_, (Option<String>,)>(
                "SELECT wallet_address FROM users WHERE id = $1"
            )
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|(w,)| w)
            .flatten()
            .unwrap_or_default();

            Json(RegisterMeterResponse {
                success: true,
                message: format!("Meter {} registered successfully. Waiting for verification.", request.serial_number),
                meter: Some(MeterResponse {
                    id: meter_id,
                    serial_number: request.serial_number,
                    meter_type,
                    location,
                    is_verified: true,
                    wallet_address: wallet,
                    latitude: request.latitude,
                    longitude: request.longitude,
                    zone_id: request.zone_id,
                }),
            })
        }
        Err(e) => {
            info!("❌ Failed to register meter: {}", e);
            Json(RegisterMeterResponse {
                success: false,
                message: format!("Failed to register meter: {}", e),
                meter: None,
            })
        }
    }
}

/// Verify a meter (mark as verified)
#[utoipa::path(
    post,
    path = "/api/v1/meters/verify",
    request_body = VerifyMeterRequest,
    responses(
        (status = 200, description = "Meter verified", body = RegisterMeterResponse),
        (status = 400, description = "Verification failed")
    ),
    tag = "meters"
)]
pub async fn verify_meter(
    State(state): State<AppState>,
    Json(request): Json<VerifyMeterRequest>,
) -> Json<RegisterMeterResponse> {
    info!("✓ Verify meter request: {}", request.serial_number);

    // Check if meter exists and owner has verified email
    let owner_check = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT u.id, u.email_verified FROM meters m 
         JOIN users u ON m.user_id = u.id 
         WHERE m.serial_number = $1"
    )
    .bind(&request.serial_number)
    .fetch_optional(&state.db)
    .await;

    match owner_check {
        Ok(Some((_, false))) => {
            info!("❌ Meter {} owner has not verified email", request.serial_number);
            return Json(RegisterMeterResponse {
                success: false,
                message: "Meter owner must verify their email before meter can be verified.".to_string(),
                meter: None,
            });
        }
        Ok(None) => {
            return Json(RegisterMeterResponse {
                success: false,
                message: format!("Meter {} not found", request.serial_number),
                meter: None,
            });
        }
        Err(e) => {
            info!("❌ Database error checking meter owner: {}", e);
            return Json(RegisterMeterResponse {
                success: false,
                message: "Database error".to_string(),
                meter: None,
            });
        }
        Ok(Some((_, true))) => {
            // Email verified, proceed with meter verification
        }
    }

    let update_result = sqlx::query(
        "UPDATE meters SET is_verified = true, updated_at = NOW() WHERE serial_number = $1"
    )
    .bind(&request.serial_number)
    .execute(&state.db)
    .await;

    match update_result {
        Ok(result) if result.rows_affected() > 0 => {
            info!("✅ Meter {} verified", request.serial_number);
            Json(RegisterMeterResponse {
                success: true,
                message: format!("Meter {} is now verified and ready to submit readings.", request.serial_number),
                meter: None,
            })
        }
        _ => {
            Json(RegisterMeterResponse {
                success: false,
                message: format!("Meter {} not found or already verified", request.serial_number),
                meter: None,
            })
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

/// Update meter status via PATCH
#[utoipa::path(
    patch,
    path = "/api/v1/meters/{serial}",
    request_body = UpdateMeterStatusRequest,
    params(
        ("serial" = String, Path, description = "Meter Serial Number")
    ),
    responses(
        (status = 200, description = "Status updated", body = RegisterMeterResponse),
        (status = 404, description = "Meter not found")
    ),
    tag = "meters"
)]
pub async fn update_meter_status(
    State(state): State<AppState>,
    axum::extract::Path(serial): axum::extract::Path<String>,
    Json(request): Json<UpdateMeterStatusRequest>,
) -> Json<RegisterMeterResponse> {
    info!("🔧 Update meter {} request: {:?}", serial, request);

    // Build dynamic query
    let mut query_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE meters SET updated_at = NOW()");
    
    if let Some(status) = &request.status {
        let is_verified = status == "verified" || status == "active";
        query_builder.push(", is_verified = ");
        query_builder.push_bind(is_verified);
    }
    
    if let Some(zone_id) = request.zone_id {
        query_builder.push(", zone_id = ");
        query_builder.push_bind(zone_id);
    }
    
    if let Some(lat) = request.latitude {
        query_builder.push(", latitude = ");
        query_builder.push_bind(lat);
    }
    
    if let Some(lng) = request.longitude {
        query_builder.push(", longitude = ");
        query_builder.push_bind(lng);
    }
    
    query_builder.push(" WHERE serial_number = ");
    query_builder.push_bind(&serial);

    let query = query_builder.build();
    let update_result = query.execute(&state.db).await;

    match update_result {
        Ok(result) if result.rows_affected() > 0 => {
            // Also sync to meter_registry if zone_id updated
            if let Some(zone_id) = request.zone_id {
                let _ = sqlx::query("UPDATE meter_registry SET zone_id = $1 WHERE meter_serial = $2")
                    .bind(zone_id)
                    .bind(&serial)
                    .execute(&state.db)
                    .await;
            }

            Json(RegisterMeterResponse {
                success: true,
                message: format!("Meter {} updated successfully", serial),
                meter: None,
            })
        }
        _ => {
            Json(RegisterMeterResponse {
                success: false,
                message: format!("Meter {} not found or no changes made", serial),
                meter: None,
            })
        }
    }
}

/// Create a new reading for a meter
/// Query params:
/// - auto_mint: If false, skip blockchain minting. Default: true
/// - timeout_secs: Blockchain operation timeout. Default: 30
#[utoipa::path(
    post,
    path = "/api/v1/meters/{serial}/readings",
    request_body = CreateReadingRequest,
    params(
        ("serial" = String, Path, description = "Meter Serial Number"),
        CreateReadingParams
    ),
    responses(
        (status = 200, description = "Reading created", body = CreateReadingResponse),
        (status = 404, description = "Meter not found")
    ),
    tag = "meters"
)]
pub async fn create_reading(
    State(state): State<AppState>,
    axum::extract::Path(serial): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<CreateReadingParams>,
    _headers: HeaderMap,
    Json(request): Json<CreateReadingRequest>,
) -> Json<CreateReadingResponse> {
    Json(internal_create_reading(&state, serial, params, request).await)
}

/// Internal shared logic for creating a reading
pub async fn internal_create_reading(
    state: &AppState,
    serial: String,
    params: CreateReadingParams,
    request: CreateReadingRequest,
) -> CreateReadingResponse {
    let reading_id = Uuid::new_v4();
    let timestamp = request.timestamp.unwrap_or_else(chrono::Utc::now);

    // 0. Oracle Validation (Sanity check before queuing)
    if let Err(e) = crate::services::validation::OracleValidator::validate_reading(
        &serial,
        &request,
        &crate::services::validation::ValidationConfig::default(),
    )
    .await
    {
        return CreateReadingResponse {
            id: reading_id,
            serial_number: serial,
            kwh: request.kwh,
            timestamp,
            minted: false,
            tx_signature: None,
            message: format!("Oracle Validation Failed: {}", e),
        };
    }

    // Push to Redis queue for asynchronous processing
    let task = crate::services::reading_processor::ReadingTask {
        serial: serial.clone(),
        params,
        request: request.clone(),
        retry_count: 0,
    };

    let (_queued, message) = match state.cache_service.push_reading(&task).await {
        Ok(_) => (true, "Reading queued for processing".to_string()),
        Err(e) => {
            error!("❌ Failed to queue reading for {}: {}", serial, e);
            (false, format!("Failed to queue reading: {}", e))
        }
    };

    CreateReadingResponse {
        id: reading_id,
        serial_number: serial,
        kwh: request.kwh,
        timestamp,
        minted: false, // Will be processed asynchronously
        tx_signature: None,
        message,
    }
}

/// Task logic for processing aqueued reading
pub async fn process_reading_task(
    state: &AppState,
    task: crate::services::reading_processor::ReadingTask,
) -> anyhow::Result<()> {
    debug!(
        "⚙️ Processing queued reading for meter {}: {} kWh",
        task.serial, task.request.kwh
    );

    let serial = task.serial;
    let params = task.params;
    let request = task.request;
    
    let auto_mint = params.auto_mint.unwrap_or(true);
    let timeout_secs = params.timeout_secs.unwrap_or(30);

    // 0. Double-check Oracle Validation in background (Secondary defense)
    if let Err(e) = crate::services::validation::OracleValidator::validate_reading(
        &serial,
        &request,
        &crate::services::validation::ValidationConfig::default(),
    )
    .await
    {
        error!("❌ Background Oracle Validation failed for {}: {}", serial, e);
        return Err(anyhow::anyhow!("Oracle Validation Failed: {}", e));
    }

    // 1. Resolve Meter Context (ID, User, Wallet, Zone)
    let (meter_id, user_id, wallet_address, zone_id) = match resolve_meter_context(state, &serial, &request.wallet_address).await {
        Ok(ctx) => ctx,
        Err(err_msg) => {
            error!("❌ Failed to resolve context for {}: {}", serial, err_msg);
            return Err(anyhow::anyhow!(err_msg));
        }
    };

    // 2. Process Blockchain Minting with Aggregation Threshold
    let (minted, tx_signature, mut _message) = if auto_mint && request.kwh > 0.0 {
        // Atomic Upsert and Increment
        let threshold = state.config.tokenization.mint_threshold;
        
        let agg_result = sqlx::query!(
            r#"
            INSERT INTO meter_unminted_balances (meter_serial, accumulated_kwh, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (meter_serial) 
            DO UPDATE SET 
                accumulated_kwh = meter_unminted_balances.accumulated_kwh + EXCLUDED.accumulated_kwh,
                updated_at = NOW()
            RETURNING accumulated_kwh
            "#,
            serial,
            request.kwh as f64
        )
        .fetch_one(&state.db)
        .await;

        match agg_result {
            Ok(row) => {
                let current_total = row.accumulated_kwh.map(|d| d.to_f64().unwrap_or(0.0)).unwrap_or(0.0);
                
                if current_total >= threshold {
                    info!("🚀 Threshold reached for {}: {} kWh >= {} kWh. Triggering mint.", serial, current_total, threshold);
                    let (m, sig, msg) = process_minting(state, timeout_secs, &wallet_address, current_total, &serial).await;
                    
                    if m {
                        // Reset balance on success
                        let _ = sqlx::query!(
                            "UPDATE meter_unminted_balances SET accumulated_kwh = 0, last_mint_at = NOW() WHERE meter_serial = $1",
                            serial
                        )
                        .execute(&state.db)
                        .await;
                        (true, sig, msg)
                    } else {
                        (false, None, format!("Threshold reached but aggregation mint failed: {}", msg))
                    }
                } else {
                    debug!("📊 Aggregating for {}: current total {} kWh (threshold: {} kWh)", serial, current_total, threshold);
                    (false, None, format!("Energy aggregated. Current total: {:.3} kWh", current_total))
                }
            },
            Err(e) => {
                error!("❌ Aggregation DB error for {}: {}", serial, e);
                (false, None, format!("Aggregation failed: {}", e))
            }
        }
    } else {
        (false, None, "Reading recorded (auto_mint disabled or negative kwh)".to_string())
    };

    // 2.5 Check for alerts and calculate health score
    let alerts = check_alerts(&serial, &request);
    if !alerts.is_empty() {
        for alert in &alerts {
            warn!("⚠️ Meter Alert: {} - {}", alert.alert_type, alert.message);
            let alert_json = serde_json::json!({
                "type": "meter_alert",
                "data": alert
            });
            state.websocket_service.broadcast_to_channel("alerts", alert_json).await;
        }
    }
    
    let health_score = calculate_health_score(&request);

    // 3. Persist Reading to Database
    let reading_id = Uuid::new_v4();
    let timestamp = request.timestamp.unwrap_or_else(chrono::Utc::now);

    if let Err(e) = persist_reading_to_db(
        state, 
        reading_id, 
        &serial, 
        meter_id, 
        user_id, 
        &wallet_address, 
        timestamp, 
        &request, 
        minted, 
        &tx_signature,
        health_score,
    ).await {
        error!("❌ CRITICAL: Failed to save reading {} to DB: {}", reading_id, e);
        return Err(anyhow::anyhow!("Database error: {}", e));
    } else {
        info!("✅ Successfully processed queued reading {} for {}", reading_id, serial);
        
        // 4. Trigger Post-Processing (Async)
        let surplus = request.surplus_energy.unwrap_or(if request.kwh > 0.0 { request.kwh } else { 0.0 });
        let deficit = request.deficit_energy.unwrap_or(if request.kwh < 0.0 { request.kwh.abs() } else { 0.0 });
        
        let power_val = request.power.or_else(|| {
             // Net power = generated - consumed
             match (request.power_generated, request.power_consumed) {
                 (Some(gen), Some(cons)) => Some(gen - cons),
                 _ => request.voltage.zip(request.current).map(|(v, i)| v * i * request.power_factor.unwrap_or(1.0) / 1000.0) // kW
             }
        });

        // Update aggregate grid status in dashboard service
        let power_gen = request.power_generated.unwrap_or(if request.kwh > 0.0 { power_val.unwrap_or(0.0) } else { 0.0 });
        let power_cons = request.power_consumed.unwrap_or(if request.kwh < 0.0 { power_val.unwrap_or(0.0).abs() } else { 0.0 });

        info!("📥 Processing power metrics for {}: gen={:.2}kW, cons={:.2}kW (raw kwh={:.4})", serial, power_gen, power_cons, request.kwh);

        let _ = state.dashboard_service.handle_meter_reading(request.kwh, &serial, zone_id, power_gen, power_cons).await;

        trigger_post_processing(
            state.clone(),
            serial.clone(),
            meter_id,
            user_id,
            surplus,
            deficit,
            request.max_sell_price,
            request.max_buy_price,
            request.kwh,
            wallet_address,
            power_val,
            request.voltage,
            request.current
        ).await;
    }

    Ok(())
}

// --- Helper Functions ---

async fn resolve_meter_context(
    state: &AppState,
    serial: &str,
    request_wallet: &Option<String>
) -> Result<(Uuid, Uuid, String, Option<i32>), String> {
    let meter_info = sqlx::query_as::<_, (Uuid, Uuid, Option<String>, Option<i32>)>(
        "SELECT m.id, m.user_id, u.wallet_address, m.zone_id FROM meter_registry m JOIN users u ON m.user_id = u.id WHERE m.meter_serial = $1"
    )
    .bind(serial)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| format!("Database lookup error: {}", e))?;
 
    match meter_info {
        Some((mid, uid, Some(w), zid)) => Ok((mid, uid, w, zid)),
        Some((mid, uid, None, zid)) => {
            if let Some(req_w) = request_wallet {
                Ok((mid, uid, req_w.clone(), zid))
            } else {
                Err("Wallet address required (not found on user profile)".to_string())
            }
        },
        None => Err("Meter not found".to_string()),
    }
}

async fn process_minting(
    state: &AppState,
    timeout_secs: u64,
    wallet_address: &str,
    kwh: f64,
    serial: &str
) -> (bool, Option<String>, String) {
    info!("🔗 Attempting blockchain mint with {}s timeout", timeout_secs);
    
    let mint_result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        async {
            // Get authority keypair
            let authority = state.wallet_service.get_authority_keypair().await
                .map_err(|e| format!("Authority keypair error: {}", e))?;
            
            // Parse addresses
            let mint_pubkey = crate::services::BlockchainService::parse_pubkey(&state.config.energy_token_mint)
                .map_err(|e| format!("Invalid token mint: {}", e))?;
            let wallet_pubkey = crate::services::BlockchainService::parse_pubkey(wallet_address)
                .map_err(|e| format!("Invalid wallet address: {}", e))?;
            
            // Ensure token account exists
            let token_account = state.blockchain_service
                .ensure_token_account_exists(&authority, &wallet_pubkey, &mint_pubkey)
                .await
                .map_err(|e| format!("Token account error: {}", e))?;
            
            // Mint tokens
            let sig = if state.config.tokenization.enable_real_blockchain {
                state.blockchain_service
                    .mint_energy_tokens(&authority, &token_account, &wallet_pubkey, &mint_pubkey, kwh)
                    .await
                    .map_err(|e| format!("Anchor Mint error: {}", e))?
            } else {
                state.blockchain_service
                    .mint_spl_tokens(&authority, &wallet_pubkey, &mint_pubkey, kwh)
                    .await
                    .map_err(|e| format!("CLI Mint error: {}", e))?
            };
            
            Ok::<_, String>(sig.to_string())
        }
    ).await;
    
    match mint_result {
        Ok(Ok(sig)) => {
            info!("🎉 Minted {} kWh for meter {} - TX: {}", kwh, serial, sig);
            (true, Some(sig), format!("{} kWh minted successfully", kwh))
        }
        Ok(Err(e)) => {
            error!("❌ Blockchain operation failed: {}", e);
            (false, None, format!("Reading recorded but minting failed: {}", e))
        }
        Err(_) => {
            error!("⏱️ Blockchain operation timed out after {}s", timeout_secs);
            (false, None, format!("Reading recorded but minting timed out after {}s", timeout_secs))
        }
    }
}

async fn persist_reading_to_db(
    state: &AppState,
    reading_id: Uuid,
    serial: &str,
    meter_id: Uuid,
    user_id: Uuid,
    wallet_address: &str,
    timestamp: chrono::DateTime<chrono::Utc>,
    request: &CreateReadingRequest,
    minted: bool,
    tx_signature: &Option<String>,
    health_score: f64,
) -> Result<(), sqlx::Error> {
    // Calculate derived energy values if not provided
    let (def_gen, def_cons) = if request.kwh > 0.0 { (request.kwh, 0.0) } else { (0.0, request.kwh.abs()) };
    
    let energy_gen = request.energy_generated.unwrap_or(def_gen);
    let energy_cons = request.energy_consumed.unwrap_or(def_cons);
    let surplus = request.surplus_energy.unwrap_or(if request.kwh > 0.0 { request.kwh } else { 0.0 });
    let deficit = request.deficit_energy.unwrap_or(if request.kwh < 0.0 { request.kwh.abs() } else { 0.0 });

    sqlx::query(
        "INSERT INTO meter_readings (
            id, meter_serial, meter_id, user_id, wallet_address, 
            timestamp, reading_timestamp, kwh_amount,
            energy_generated, energy_consumed, surplus_energy, deficit_energy,
            voltage, current_amps, power_factor, frequency, temperature,
            thd_voltage, thd_current,
            latitude, longitude, battery_level, weather_condition, health_score,
            rec_eligible, carbon_offset, max_sell_price, max_buy_price,
            meter_signature, meter_type,
            minted, mint_tx_signature, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8, $9, $10, $11, 
                   $12, $13, $14, $15, $16, $17, $18, 
                   $19, $20, $21, $22, $23,
                   $24, $25, $26, $27, $28, $29, $30, $31, NOW())"
    )
    .bind(reading_id)
    .bind(serial)
    .bind(meter_id)
    .bind(user_id)
    .bind(wallet_address)
    .bind(timestamp)
    .bind(request.kwh)
    .bind(energy_gen)
    .bind(energy_cons)
    .bind(surplus)
    .bind(deficit)
    // Electrical parameters
    .bind(request.voltage)
    .bind(request.current)
    .bind(request.power_factor)
    .bind(request.frequency)
    .bind(request.temperature)
    // THD
    .bind(request.thd_voltage)
    .bind(request.thd_current)
    // GPS
    .bind(request.latitude)
    .bind(request.longitude)
    // Battery & Environmental
    .bind(request.battery_level)
    .bind(&request.weather_condition)
    // Health
    .bind(health_score)
    // Trading
    .bind(request.rec_eligible.unwrap_or(false))
    .bind(request.carbon_offset)
    .bind(request.max_sell_price)
    .bind(request.max_buy_price)
    // Security
    .bind(&request.meter_signature)
    .bind(&request.meter_type)
    // Minting status
    .bind(minted)
    .bind(tx_signature.clone())
    .execute(&state.db)
    .await
    .map(|_| ())
}

async fn trigger_post_processing(
    state: AppState,
    serial: String,
    meter_id: Uuid,
    user_id: Uuid,
    surplus: f64,
    deficit: f64,
    max_sell_price: Option<f64>,
    max_buy_price: Option<f64>,
    kwh: f64,
    wallet_address: String,
    power: Option<f64>,
    voltage: Option<f64>,
    current: Option<f64>
) {
    let _db = state.db.clone();
    let websocket = state.websocket_service.clone();
    
    // Broadcast real-time meter update
    let ws_meter_serial = serial.clone();
    let ws_wallet = wallet_address.clone();
    tokio::spawn(async move {
        websocket.broadcast_meter_reading_received(
            &user_id,
            &ws_wallet,
            &ws_meter_serial,
            kwh,
            power,
            voltage,
            current
        ).await;
    });

    // P2P Auto-Order Generation
    let market_clearing = state.market_clearing.clone();
    let surplus_val = rust_decimal::Decimal::from_f64_retain(surplus).unwrap_or_default();
    let deficit_val = rust_decimal::Decimal::from_f64_retain(deficit).unwrap_or_default();
    
    let sell_price = max_sell_price.map(|p| rust_decimal::Decimal::from_f64_retain(p).unwrap_or_default());
    let buy_price = max_buy_price.map(|p| rust_decimal::Decimal::from_f64_retain(p).unwrap_or_default());

    tokio::spawn(async move {
        // Handle Surplus -> Sell Order
        if surplus_val > rust_decimal::Decimal::ZERO {
            match sell_price {
                Some(price) if price > rust_decimal::Decimal::ZERO => {
                    info!("📈 [Auto-P2P] Triggering SELL order for meter {}: {} kWh @ {} THB", serial, surplus_val, price);
                    let res = market_clearing.create_order(
                        user_id,
                        crate::database::schema::types::OrderSide::Sell,
                        crate::database::schema::types::OrderType::Limit,
                        surplus_val,
                        Some(price),
                        None,
                        None,
                        Some(meter_id),
                        None,
                    ).await;
                    if let Err(e) = res {
                        error!("❌ [Auto-P2P] Failed to create Sell order for {}: {}", serial, e);
                    }
                }
                _ => {} // No price preference, skip
            }
        }

        // Handle Deficit -> Buy Order
        if deficit_val > rust_decimal::Decimal::ZERO {
            match buy_price {
                Some(price) if price > rust_decimal::Decimal::ZERO => {
                    info!("📉 [Auto-P2P] Triggering BUY order for meter {}: {} kWh @ {} THB", serial, deficit_val, price);
                    let res = market_clearing.create_order(
                        user_id,
                        crate::database::schema::types::OrderSide::Buy,
                        crate::database::schema::types::OrderType::Limit,
                        deficit_val,
                        Some(price),
                        None,
                        None,
                        Some(meter_id),
                        None,
                    ).await;
                    if let Err(e) = res {
                        error!("❌ [Auto-P2P] Failed to create Buy order for {}: {}", serial, e);
                    }
                }
                _ => {} // No price preference, skip
            }
        }
    });
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

/// Create multiple readings in a single batch
#[utoipa::path(
    post,
    path = "/api/v1/meters/batch/readings",
    request_body = CreateBatchReadingRequest,
    responses(
        (status = 200, description = "Batch processed", body = BatchReadingResponse)
    ),
    tag = "meters"
)]
pub async fn create_batch_readings(
    State(state): State<AppState>,
    Json(request): Json<CreateBatchReadingRequest>,
) -> Json<BatchReadingResponse> {
    let mut success_count = 0;
    let mut failed_count = 0;
    
    info!("📊 Processing batch of {} readings", request.readings.len());
    
    let futures = request.readings.into_iter().map(|reading| {
        let state = state.clone();
        async move {
            let serial = reading.meter_serial.clone().or_else(|| reading.meter_id.clone());
            if let Some(serial) = serial {
                let params = CreateReadingParams {
                    auto_mint: Some(true),
                    timeout_secs: Some(30),
                };
                let _ = internal_create_reading(&state, serial, params, reading).await;
                Ok::<_, ()>(true)
            } else {
                Ok::<_, ()>(false)
            }
        }
    });

    let results = futures::future::join_all(futures).await;
    
    for res in results {
        match res {
            Ok(true) => success_count += 1,
            _ => failed_count += 1,
        }
    }
    
    Json(BatchReadingResponse {
        success_count,
        failed_count,
        message: format!("Processed {} readings ({} failed)", success_count + failed_count, failed_count),
    })
}
