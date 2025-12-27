//! Meters Handlers Module
//!
//! Meter management handlers: registration, verification, readings, etc.

use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use tracing::{info, error};
use uuid::Uuid;

use crate::AppState;
use super::types::{
    MeterResponse, PublicMeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    VerifyMeterRequest, MeterFilterParams, UpdateMeterStatusRequest,
    CreateReadingRequest, CreateReadingResponse, MeterReadingResponse, ReadingFilterParams,
    CreateReadingParams, MeterStats,
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
        let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>)>(
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude
             FROM meters m
             JOIN users u ON m.user_id = u.id
             WHERE m.user_id = $1"
        )
        .bind(claims.sub)
        .fetch_all(&state.db)
        .await;

        if let Ok(meters) = meters_result {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
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
    
    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>)>(
        "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude
         FROM meters m
         JOIN users u ON m.user_id = u.id
         WHERE m.is_verified = true"
    )
    .fetch_all(&state.db)
    .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
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
    
    // Query public-safe fields with latest reading data
    let meters_result = sqlx::query_as::<_, (String, String, bool, Option<f64>, Option<f64>, Option<f64>, Option<f64>)>(
        r#"
        SELECT 
            m.meter_type, 
            m.location, 
            m.is_verified, 
            m.latitude, 
            m.longitude,
            lr.current_generation,
            lr.current_consumption
        FROM meters m
        LEFT JOIN LATERAL (
            SELECT 
                energy_generated::FLOAT8 as current_generation, 
                energy_consumed::FLOAT8 as current_consumption
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
            let responses: Vec<PublicMeterResponse> = meters.iter().map(|(mtype, loc, verified, lat, lng, gen, cons)| {
                PublicMeterResponse {
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    latitude: *lat,
                    longitude: *lng,
                    current_generation: *gen,
                    current_consumption: *cons,
                }
            }).collect();
            
            info!("✅ Public API: Returning {} meters for map (with latest readings)", responses.len());
            Json(responses)
        }
        Err(e) => {
            error!("❌ Public meters error: {}", e);
            Json(vec![])
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
    headers: HeaderMap,
    Json(request): Json<RegisterMeterRequest>,
) -> Json<RegisterMeterResponse> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    info!("📊 Register meter request: {}", request.serial_number);

    // Verify user token
    let claims = match state.jwt_service.decode_token(token) {
        Ok(c) => c,
        Err(_) => {
            return Json(RegisterMeterResponse {
                success: false,
                message: "Invalid or expired token. Please login again.".to_string(),
                meter: None,
            });
        }
    };

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

    // Insert meter into database with coordinates
    let insert_result = sqlx::query(
        "INSERT INTO meters (id, user_id, serial_number, meter_type, location, latitude, longitude, is_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, true, NOW(), NOW())"
    )
    .bind(meter_id)
    .bind(user_id)
    .bind(&request.serial_number)
    .bind(&meter_type)
    .bind(&location)
    .bind(request.latitude)
    .bind(request.longitude)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Meter {} registered for user {}", request.serial_number, user_id);
            
            // Sync to meter_registry for FK constraints
            let _ = sqlx::query(
                "INSERT INTO meter_registry (id, user_id, meter_serial, meter_type, location_address, meter_key_hash, verification_method, verification_status)
                 VALUES ($1, $2, $3, $4, $5, 'mock_hash', 'serial', 'verified')"
            )
            .bind(meter_id)
            .bind(user_id)
            .bind(&request.serial_number)
            .bind(&meter_type)
            .bind(&location)
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
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = true"
        }
        Some("pending") => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = false"
        }
        _ => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address, m.latitude, m.longitude
             FROM meters m JOIN users u ON m.user_id = u.id"
        }
    };

    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>, Option<f64>, Option<f64>)>(query)
        .fetch_all(&state.db)
        .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet, lat, lng)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                    latitude: *lat,
                    longitude: *lng,
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
    info!("🔧 Update meter {} status to: {}", serial, request.status);

    let is_verified = request.status == "verified" || request.status == "active";
    
    let update_result = sqlx::query(
        "UPDATE meters SET is_verified = $1, updated_at = NOW() WHERE serial_number = $2"
    )
    .bind(is_verified)
    .bind(&serial)
    .execute(&state.db)
    .await;

    match update_result {
        Ok(result) if result.rows_affected() > 0 => {
            Json(RegisterMeterResponse {
                success: true,
                message: format!("Meter {} status updated to '{}'", serial, request.status),
                meter: None,
            })
        }
        _ => {
            Json(RegisterMeterResponse {
                success: false,
                message: format!("Meter {} not found", serial),
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
    let auto_mint = params.auto_mint.unwrap_or(true);
    let timeout_secs = params.timeout_secs.unwrap_or(30);
    
    info!(
        "📊 Create reading for meter {}: {} kWh (auto_mint={}, timeout={}s)",
        serial, request.kwh, auto_mint, timeout_secs
    );

    // Get wallet address, meter_id, user_id from meter or request
    let (meter_id, user_id, wallet_address) = {
        let meter_info = sqlx::query_as::<_, (Uuid, Uuid, Option<String>)>(
            "SELECT m.id, m.user_id, u.wallet_address FROM meter_registry m JOIN users u ON m.user_id = u.id WHERE m.meter_serial = $1"
        )
        .bind(&serial)
        .fetch_optional(&state.db)
        .await;

        match meter_info {
            Ok(Some((mid, uid, Some(w)))) => (mid, uid, w),
            Ok(Some((mid, uid, None))) => {
                if let Some(req_w) = request.wallet_address.clone() {
                    (mid, uid, req_w)
                } else {
                     return Json(CreateReadingResponse {
                        id: Uuid::new_v4(),
                        serial_number: serial,
                        kwh: request.kwh,
                        timestamp: request.timestamp.unwrap_or_else(chrono::Utc::now),
                        minted: false,
                        tx_signature: None,
                        message: "Wallet address required (not found on user profile)".to_string(),
                    });
                }
            }
            _ => {
                return Json(CreateReadingResponse {
                    id: Uuid::new_v4(),
                    serial_number: serial,
                    kwh: request.kwh,
                    timestamp: request.timestamp.unwrap_or_else(chrono::Utc::now),
                    minted: false,
                    tx_signature: None,
                    message: "Meter not found".to_string(),
                });
            }
        }
    };

    let reading_id = Uuid::new_v4();
    let timestamp = request.timestamp.unwrap_or_else(chrono::Utc::now);

    // Track minting result
    let mut minted = false;
    let mut tx_signature: Option<String> = None;
    let mut message = "Reading recorded".to_string();

    // Only attempt minting if auto_mint is enabled and kwh is positive
    if auto_mint && request.kwh > 0.0 {
        info!("🔗 Attempting blockchain mint with {}s timeout", timeout_secs);
        
        // Wrap blockchain operations in a timeout
        let mint_result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            async {
                // Get authority keypair
                let authority = state.wallet_service.get_authority_keypair().await
                    .map_err(|e| format!("Authority keypair error: {}", e))?;
                
                // Parse addresses
                let mint_pubkey = crate::services::BlockchainService::parse_pubkey(&state.config.energy_token_mint)
                    .map_err(|e| format!("Invalid token mint: {}", e))?;
                let wallet_pubkey = crate::services::BlockchainService::parse_pubkey(&wallet_address)
                    .map_err(|e| format!("Invalid wallet address: {}", e))?;
                
                // Ensure token account exists
                let token_account = state.blockchain_service
                    .ensure_token_account_exists(&authority, &wallet_pubkey, &mint_pubkey)
                    .await
                    .map_err(|e| format!("Token account error: {}", e))?;
                
                // Mint tokens
                let sig = if state.config.tokenization.enable_real_blockchain {
                    state.blockchain_service
                        .mint_energy_tokens(&authority, &token_account, &wallet_pubkey, &mint_pubkey, request.kwh)
                        .await
                        .map_err(|e| format!("Anchor Mint error: {}", e))?
                } else {
                    state.blockchain_service
                        .mint_spl_tokens(&authority, &wallet_pubkey, &mint_pubkey, request.kwh)
                        .await
                        .map_err(|e| format!("CLI Mint error: {}", e))?
                };
                
                Ok::<_, String>(sig.to_string())
            }
        ).await;
        
        match mint_result {
            Ok(Ok(sig)) => {
                minted = true;
                tx_signature = Some(sig.clone());
                if request.kwh > 0.0 {
                    message = format!("{} kWh minted successfully", request.kwh);
                    info!("🎉 Minted {} kWh for meter {} - TX: {}", request.kwh, serial, sig);
                } else {
                    message = format!("{} kWh burned successfully", request.kwh.abs());
                    info!("🔥 Burned {} kWh for meter {} - TX: {}", request.kwh.abs(), serial, sig);
                }
            }
            Ok(Err(e)) => {
                error!("❌ Blockchain operation failed: {}", e);
                message = format!("Reading recorded but minting failed: {}", e);
            }
            Err(_) => {
                error!("⏱️ Blockchain operation timed out after {}s", timeout_secs);
                message = format!("Reading recorded but minting timed out after {}s", timeout_secs);
            }
        }
    } else if !auto_mint {
        message = "Reading recorded (auto_mint disabled)".to_string();
        info!("📝 Reading saved without minting (auto_mint=false)");
    }

    // Persist reading to database with full telemetry
    let (gen, cons) = if request.kwh > 0.0 { (request.kwh, 0.0) } else { (0.0, request.kwh.abs()) };
    
    // Use request values for energy if provided, otherwise calculate from kwh
    let energy_gen = request.energy_generated.unwrap_or(gen);
    let energy_cons = request.energy_consumed.unwrap_or(cons);
    let surplus = request.surplus_energy.unwrap_or(if request.kwh > 0.0 { request.kwh } else { 0.0 });
    let deficit = request.deficit_energy.unwrap_or(if request.kwh < 0.0 { request.kwh.abs() } else { 0.0 });

    let insert_result = sqlx::query(
        "INSERT INTO meter_readings (
            id, meter_serial, meter_id, user_id, wallet_address, 
            timestamp, reading_timestamp, kwh_amount,
            energy_generated, energy_consumed, surplus_energy, deficit_energy,
            voltage, current_amps, power_factor, frequency, temperature,
            latitude, longitude, battery_level, weather_condition,
            rec_eligible, carbon_offset, max_sell_price, max_buy_price,
            meter_signature, meter_type,
            minted, mint_tx_signature, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8, $9, $10, $11, 
                   $12, $13, $14, $15, $16, $17, $18, $19, $20,
                   $21, $22, $23, $24, $25, $26, $27, $28, NOW())"
    )
    .bind(reading_id)
    .bind(&serial)
    .bind(meter_id)
    .bind(user_id)
    .bind(&wallet_address)
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
    // GPS
    .bind(request.latitude)
    .bind(request.longitude)
    // Battery & Environmental
    .bind(request.battery_level)
    .bind(&request.weather_condition)
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
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Successfully saved reading {} to DB", reading_id);
        }
        Err(e) => {
            error!("❌ CRITICAL: Failed to save reading {} to DB: {}", reading_id, e);
            message = format!("{}. Database error: {}", message, e);
        }
    }

    Json(CreateReadingResponse {
        id: reading_id,
        serial_number: serial,
        kwh: request.kwh,
        timestamp,
        minted,
        tx_signature,
        message,
    })
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
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<ReadingFilterParams>,
) -> Json<Vec<MeterReadingResponse>> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    info!("📊 Get readings request");

    if let Ok(claims) = state.jwt_service.decode_token(token) {
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
    headers: HeaderMap,
) -> Json<MeterStats> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    if let Ok(claims) = state.jwt_service.decode_token(token) {
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
    }
    
    Json(MeterStats::default())
}
