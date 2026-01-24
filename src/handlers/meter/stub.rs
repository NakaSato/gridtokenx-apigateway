//! Simplified Meter Stub Handler
//! 
//! This is a minimal meter reading handler that bypasses SQLx compile-time checking
//! by storing readings in memory and triggering blockchain operations directly.

use axum::{
    extract::{State, Path, Query},
    Json,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;
use serde_json;

use crate::{
    error::{ApiError, Result},
    services::{BlockchainService, meter_analyzer::{check_alerts, calculate_health_score}},
    handlers::meter::types::SubmitReadingRequest,
    AppState,
};

/// Response after submitting a reading
#[derive(Debug, Serialize)]
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub wallet_address: String,
    pub kwh_amount: Decimal,
    pub reading_timestamp: DateTime<Utc>,
    pub submitted_at: DateTime<Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
    pub message: String,
}

/// Query parameters for getting meter readings
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GetReadingsQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Single reading record for query response
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ReadingRecord {
    pub id: Uuid,
    pub meter_serial: String,
    pub timestamp: DateTime<Utc>,
    pub kwh_amount: f64,
    pub energy_generated: Option<f64>,
    pub energy_consumed: Option<f64>,
    pub voltage: Option<f64>,
    pub current_amps: Option<f64>,
    pub power_factor: Option<f64>,
    pub frequency: Option<f64>,
    pub temperature: Option<f64>,
    pub thd_voltage: Option<f64>,
    pub thd_current: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub battery_level: Option<f64>,
    pub health_score: Option<f64>,
    pub minted: bool,
}

/// Response for readings query
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadingsResponse {
    pub readings: Vec<ReadingRecord>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Get meter readings with filters
#[utoipa::path(
    get,
    path = "/api/v1/meters/{serial}/readings",
    params(
        ("serial" = String, Path, description = "Meter serial number"),
        GetReadingsQuery
    ),
    responses(
        (status = 200, description = "List of meter readings", body = ReadingsResponse),
        (status = 404, description = "Meter not found")
    ),
    tag = "meters"
)]
pub async fn get_meter_readings(
    State(state): State<AppState>,
    Path(serial): Path<String>,
    Query(params): Query<GetReadingsQuery>,
) -> Result<Json<ReadingsResponse>> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);

    // Build query with optional date filters
    let readings = if let (Some(from), Some(to)) = (params.from, params.to) {
        sqlx::query_as::<_, ReadingRecord>(
            r#"SELECT id, meter_serial, timestamp, kwh_amount, 
                      energy_generated, energy_consumed, voltage, current_amps,
                      power_factor, frequency, temperature, thd_voltage, thd_current,
                      latitude, longitude, battery_level, health_score, minted
               FROM meter_readings 
               WHERE meter_serial = $1 AND timestamp >= $2 AND timestamp <= $3
               ORDER BY timestamp DESC
               LIMIT $4 OFFSET $5"#
        )
        .bind(&serial)
        .bind(from)
        .bind(to)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::Database)?
    } else if let Some(from) = params.from {
        sqlx::query_as::<_, ReadingRecord>(
            r#"SELECT id, meter_serial, timestamp, kwh_amount, 
                      energy_generated, energy_consumed, voltage, current_amps,
                      power_factor, frequency, temperature, thd_voltage, thd_current,
                      latitude, longitude, battery_level, health_score, minted
               FROM meter_readings 
               WHERE meter_serial = $1 AND timestamp >= $2
               ORDER BY timestamp DESC
               LIMIT $3 OFFSET $4"#
        )
        .bind(&serial)
        .bind(from)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::Database)?
    } else {
        sqlx::query_as::<_, ReadingRecord>(
            r#"SELECT id, meter_serial, timestamp, kwh_amount, 
                      energy_generated, energy_consumed, voltage, current_amps,
                      power_factor, frequency, temperature, thd_voltage, thd_current,
                      latitude, longitude, battery_level, health_score, minted
               FROM meter_readings 
               WHERE meter_serial = $1
               ORDER BY timestamp DESC
               LIMIT $2 OFFSET $3"#
        )
        .bind(&serial)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::Database)?
    };

    // Get total count
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meter_readings WHERE meter_serial = $1"
    )
    .bind(&serial)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(ReadingsResponse {
        readings,
        total,
        limit,
        offset,
    }))
}

/// Query parameters for historical trends
pub type GetTrendsQuery = crate::handlers::auth::types::GetTrendsQuery;

/// Record for trend aggregation
pub type TrendRecord = crate::handlers::auth::types::TrendRecord;

/// Response for historical trends
pub type TrendResponse = crate::handlers::auth::types::TrendResponse;

/// Get aggregated energy trends for a meter
#[utoipa::path(
    get,
    path = "/api/v1/meters/{serial}/trends",
    params(
        ("serial" = String, Path, description = "Meter serial number"),
        GetTrendsQuery
    ),
    responses(
        (status = 200, description = "Aggregated energy trends", body = TrendResponse),
        (status = 404, description = "Meter not found")
    ),
    tag = "meters"
)]
pub async fn get_meter_trends(
    State(state): State<AppState>,
    Path(serial): Path<String>,
    Query(params): Query<GetTrendsQuery>,
) -> Result<Json<TrendResponse>> {
    let period = params.period.unwrap_or_else(|| "day".to_string());
    let now = Utc::now();
    let from = params.from.unwrap_or_else(|| now - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(|| now);

    let interval = match period.as_str() {
        "hour" => "hour",
        "month" => "month",
        _ => "day",
    };

    let data = sqlx::query_as::<_, TrendRecord>(
        r#"SELECT 
            DATE_TRUNC($1, timestamp) as time_bucket,
            SUM(energy_generated) as production,
            SUM(energy_consumed) as consumption,
            SUM(kwh_amount) as net_energy,
            AVG(health_score) as avg_health
           FROM meter_readings
           WHERE meter_serial = $2 AND timestamp >= $3 AND timestamp <= $4
           GROUP BY time_bucket
           ORDER BY time_bucket ASC"#
    )
    .bind(interval)
    .bind(&serial)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok(Json(TrendResponse {
        meter_serial: serial,
        period: interval.to_string(),
        data,
    }))
}

// ============================================================================
// ALERTS AND HEALTH SCORING
// ============================================================================

/// Health response for a meter
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeterHealthResponse {
    pub meter_serial: String,
    pub health_score: f64,
    pub status: String,
    pub last_reading: Option<DateTime<Utc>>,
    pub components: HealthComponents,
}

/// Individual health component scores
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HealthComponents {
    pub voltage_stability: Option<f64>,
    pub power_factor: Option<f64>,
    pub thd_quality: Option<f64>,
    pub battery_level: Option<f64>,
}

/// Get meter health status
#[utoipa::path(
    get,
    path = "/api/v1/meters/{serial}/health",
    params(
        ("serial" = String, Path, description = "Meter serial number")
    ),
    responses(
        (status = 200, description = "Meter health status", body = MeterHealthResponse),
        (status = 404, description = "Meter not found")
    ),
    tag = "meters"
)]
pub async fn get_meter_health(
    State(state): State<AppState>,
    Path(serial): Path<String>,
) -> Result<Json<MeterHealthResponse>> {
    // Get latest reading for health calculation
    let latest = sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>, DateTime<Utc>)>(
        r#"SELECT voltage, power_factor, thd_voltage, thd_current, battery_level, timestamp
           FROM meter_readings WHERE meter_serial = $1 
           ORDER BY timestamp DESC LIMIT 1"#
    )
    .bind(&serial)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    match latest {
        Some((voltage, power_factor, thd_v, thd_i, battery, timestamp)) => {
            // Calculate component scores
            let voltage_score = voltage.map(|v| {
                if v >= 220.0 && v <= 240.0 { 100.0 }
                else if v >= 200.0 && v <= 260.0 { 75.0 }
                else { 25.0 }
            });
            let pf_score = power_factor.map(|pf| pf * 100.0);
            let thd_score = match (thd_v, thd_i) {
                (Some(v), Some(i)) => Some((100.0 - (v + i) * 5.0).max(0.0)),
                (Some(v), None) => Some((100.0 - v * 10.0).max(0.0)),
                _ => None,
            };

            // Calculate overall score
            let mut total = 0.0;
            let mut count = 0;
            if let Some(s) = voltage_score { total += s; count += 1; }
            if let Some(s) = pf_score { total += s; count += 1; }
            if let Some(s) = thd_score { total += s; count += 1; }
            if let Some(s) = battery { total += s; count += 1; }

            let health_score = if count > 0 { total / count as f64 } else { 50.0 };

            let status = if health_score >= 80.0 { "Good" }
                else if health_score >= 60.0 { "Fair" }
                else if health_score >= 40.0 { "Poor" }
                else { "Critical" };

            Ok(Json(MeterHealthResponse {
                meter_serial: serial,
                health_score,
                status: status.to_string(),
                last_reading: Some(timestamp),
                components: HealthComponents {
                    voltage_stability: voltage_score,
                    power_factor: pf_score,
                    thd_quality: thd_score,
                    battery_level: battery,
                },
            }))
        }
        None => {
            Ok(Json(MeterHealthResponse {
                meter_serial: serial,
                health_score: 0.0,
                status: "Unknown".to_string(),
                last_reading: None,
                components: HealthComponents {
                    voltage_stability: None,
                    power_factor: None,
                    thd_quality: None,
                    battery_level: None,
                },
            }))
        }
    }
}

/// Submit a new meter reading (simplified, no database)
/// POST /submit-reading
pub async fn submit_reading(
    State(state): State<AppState>,
    Json(request): Json<SubmitReadingRequest>,
) -> Result<Json<MeterReadingResponse>> {
    info!(
        "📊 Received meter reading: {} kWh for wallet {:?}",
        request.kwh_amount, request.wallet_address
    );

    // Get wallet address from request (required for simulator)
    let wallet_address = request.wallet_address.clone().ok_or_else(|| {
        ApiError::BadRequest("Wallet address required".to_string())
    })?;

    // Generate a reading ID (in real implementation this would be from database)
    let reading_id = Uuid::new_v4();
    let submitted_at = Utc::now();

    // Validate the reading
    let kwh_f64 = request.kwh_amount.to_f64().unwrap_or(0.0);
    
    if kwh_f64.abs() > 100.0 {
        return Err(ApiError::BadRequest("kWh amount exceeds maximum (100 kWh)".to_string()));
    }

    info!("✅ Reading validated. ID: {}, Amount: {} kWh", reading_id, kwh_f64);

    // Validate meter is registered (if meter_serial provided)
    let mut zone_id = None;
    if let Some(ref meter_serial) = request.meter_serial {
        let meter_info = sqlx::query!(
            "SELECT count(*) as count, zone_id FROM meters WHERE serial_number = $1 GROUP BY zone_id",
            meter_serial
        )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        match meter_info {
            Some(record) if record.count.unwrap_or(0) > 0 => {
                info!("✅ Meter {} is registered in Zone {:?}", meter_serial, record.zone_id);
                zone_id = record.zone_id;
            },
            _ => {
                warn!("⚠️ Meter {} not registered, rejecting reading", meter_serial);
                return Err(ApiError::NotFound(format!("Meter {} is not registered. Please register the meter first.", meter_serial)));
            }
        }
    }

    // Update aggregate grid status in dashboard service immediately after validation
    let power_gen = request.power_generated.unwrap_or(0.0);
    let power_cons = request.power_consumed.unwrap_or(0.0);
    
    let _ = state.dashboard_service.handle_meter_reading(
        kwh_f64, 
        request.meter_serial.as_deref().unwrap_or("unknown"), 
        zone_id,
        power_gen,
        power_cons
    ).await;

    // Check for alerts and broadcast via WebSocket
    let meter_id = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
    let alerts = check_alerts(&meter_id, &request);
    if !alerts.is_empty() {
        for alert in &alerts {
            warn!("⚠️ Alert: {} - {}", alert.alert_type, alert.message);
            // Broadcast alert via WebSocket
            let alert_json = serde_json::json!({
                "type": "meter_alert",
                "data": alert
            });
            state.websocket_service.broadcast_to_channel("alerts", alert_json).await;
        }
        info!("📢 Broadcast {} alerts for meter {}", alerts.len(), meter_id);
    }

    // Calculate health score
    let health_score = calculate_health_score(&request);
    info!("📊 Health score for {}: {:.1}", meter_id, health_score);

    // Track minting result
    let mut minted = false;
    let mut mint_tx_signature: Option<String> = None;
    let mut message = "Reading received".to_string();

    // Attempt blockchain minting if amount is positive
    if kwh_f64 > 0.0 {
        info!("🔗 Triggering blockchain mint for {} kWh", kwh_f64);

        // Get authority keypair
        match state.wallet_service.get_authority_keypair().await {
            Ok(authority_keypair) => {
                info!("✅ Authority keypair loaded");
                
                // Parse addresses
                let token_mint_result = BlockchainService::parse_pubkey(&state.config.energy_token_mint);
                let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

                match (token_mint_result, wallet_pubkey_result) {
                    (Ok(token_mint), Ok(wallet_pubkey)) => {
                        info!("✅ Parsed token mint and wallet pubkey");
                        
                        // Ensure token account exists
                        match state
                            .blockchain_service
                            .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                            .await
                        {
                            Ok(user_token_account) => {
                                info!("✅ Token account exists: {}", user_token_account);
                                
                                // Get meter serial for on-chain update
                                let meter_serial = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
                                
                                // Convert kWh to Wh for on-chain storage (u64)
                                let energy_wh = (kwh_f64 * 1000.0) as u64;
                                let reading_timestamp = request.reading_timestamp.timestamp();
                                
                                // Step 1: Update on-chain meter reading via Registry program
                                // Note: Authority must be set as oracle via set_oracle_authority on Registry
                                let registry_update_result = state
                                    .blockchain_service
                                    .update_meter_reading_on_chain(
                                        &authority_keypair,
                                        &meter_serial,
                                        energy_wh,  // energy_generated (for generation readings)
                                        0,          // energy_consumed (0 for generation)
                                        reading_timestamp,
                                    )
                                    .await;
                                
                                match registry_update_result {
                                    Ok(registry_sig) => {
                                        info!("📝 Registry updated on-chain: {}", registry_sig);
                                    }
                                    Err(e) => {
                                        // Log but continue - registry update is optional for now
                                        // This allows graceful degradation if oracle not configured
                                        warn!("⚠️ On-chain registry update failed (continuing): {}", e);
                                    }
                                }
                                
                                // Step 2: Mint tokens (auto-minting)
                                let mint_result = state
                                    .blockchain_service
                                    .mint_energy_tokens(
                                        &authority_keypair,
                                        &user_token_account,
                                        &wallet_pubkey,
                                        &token_mint,
                                        kwh_f64,
                                    )
                                    .await;

                                match mint_result {
                                    Ok(signature) => {
                                        let sig_str = signature.to_string();
                                        info!("🎉 Mint successful! Signature: {}", sig_str);
                                        minted = true;
                                        mint_tx_signature = Some(sig_str.clone());
                                        message = format!("Reading received and {} kWh minted. TX: {}", kwh_f64, sig_str);
                                        
                                        // Broadcast meter reading received via WebSocket
                                        let power = request.energy_generated.unwrap_or(0.0) - request.energy_consumed.unwrap_or(0.0);
                                        let _ = state
                                            .websocket_service
                                            .broadcast_meter_reading_received(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                kwh_f64,
                                                Some(power),
                                                request.voltage,
                                                request.current,
                                            )
                                            .await;
                                        
                                        // Broadcast tokens minted via WebSocket
                                        let tokens_minted = (kwh_f64 * 1_000_000_000.0) as u64;
                                        let _ = state
                                            .websocket_service
                                            .broadcast_tokens_minted(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                kwh_f64,
                                                tokens_minted,
                                                &sig_str,
                                            )
                                            .await;
                                    }
                                    Err(e) => {
                                        error!("❌ Mint failed: {}", e);
                                        message = format!("Reading received but minting failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to ensure token account exists: {}", e);
                                message = format!("Reading received but token account creation failed: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("❌ Invalid token mint or wallet address");
                        message = "Reading received but invalid addresses".to_string();
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ Authority keypair not available - skipping blockchain action: {}", e);
                message = format!("Reading received but authority wallet not available: {}", e);
            }
        }
    } else if kwh_f64 < 0.0 {
        // Consumption - burn tokens
        let burn_amount = kwh_f64.abs();
        info!("🔥 Triggering token burn for {} kWh consumption", burn_amount);

        // Get authority keypair
        match state.wallet_service.get_authority_keypair().await {
            Ok(authority_keypair) => {
                info!("✅ Authority keypair loaded for burn");
                
                // Parse addresses
                let token_mint_result = BlockchainService::parse_pubkey(&state.config.energy_token_mint);
                let wallet_pubkey_result = BlockchainService::parse_pubkey(&wallet_address);

                match (token_mint_result, wallet_pubkey_result) {
                    (Ok(token_mint), Ok(wallet_pubkey)) => {
                        info!("✅ Parsed token mint and wallet pubkey for burn");
                        
                        // Get user's token account
                        match state
                            .blockchain_service
                            .ensure_token_account_exists(&authority_keypair, &wallet_pubkey, &token_mint)
                            .await
                        {
                            Ok(user_token_account) => {
                                info!("✅ Token account exists: {}", user_token_account);
                                
                                // Get meter serial for on-chain update
                                let meter_serial = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
                                
                                // Convert kWh to Wh for on-chain storage (u64)
                                let energy_wh = (burn_amount * 1000.0) as u64;
                                let reading_timestamp = request.reading_timestamp.timestamp();
                                
                                // Step 1: Update on-chain meter reading via Registry program
                                // For consumption, energy_generated=0, energy_consumed=energy_wh
                                let registry_update_result = state
                                    .blockchain_service
                                    .update_meter_reading_on_chain(
                                        &authority_keypair,
                                        &meter_serial,
                                        0,          // energy_generated (0 for consumption)
                                        energy_wh,  // energy_consumed
                                        reading_timestamp,
                                    )
                                    .await;
                                
                                match registry_update_result {
                                    Ok(registry_sig) => {
                                        info!("📝 Registry updated on-chain (consumption): {}", registry_sig);
                                    }
                                    Err(e) => {
                                        warn!("⚠️ On-chain registry update failed (continuing): {}", e);
                                    }
                                }
                                
                                // Step 2: Burn tokens
                                let burn_result = state
                                    .blockchain_service
                                    .burn_energy_tokens(
                                        &authority_keypair,
                                        &user_token_account,
                                        &token_mint,
                                        burn_amount,
                                    )
                                    .await;

                                match burn_result {
                                    Ok(signature) => {
                                        let sig_str = signature.to_string();
                                        info!("🔥 Burn successful! Signature: {}", sig_str);
                                        minted = false; // Not minted, it was burned
                                        mint_tx_signature = Some(sig_str.clone());
                                        message = format!("Consumption of {} kWh recorded. {} tokens burned. TX: {}", burn_amount, burn_amount, sig_str);
                                        
                                        // Broadcast consumption event via WebSocket
                                        let _ = state
                                            .websocket_service
                                            .broadcast_meter_reading_received(
                                                &Uuid::nil(),
                                                &wallet_address,
                                                request.meter_serial.as_deref().unwrap_or("unknown"),
                                                -burn_amount, // Negative to indicate consumption
                                                Some(-burn_amount), // power (negative for consumption)
                                                request.voltage,
                                                request.current,
                                            )
                                            .await;
                                    }
                                    Err(e) => {
                                        error!("❌ Burn failed: {}", e);
                                        message = format!("Consumption recorded but burn failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to get token account for burn: {}", e);
                                message = format!("Consumption recorded but token account not found: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("❌ Invalid token mint or wallet address for burn");
                        message = "Consumption recorded but invalid addresses".to_string();
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ Authority keypair not available for burn: {}", e);
                message = format!("Consumption recorded but authority wallet not available: {}", e);
            }
        }
    }

    // Store reading to database with all telemetry data
    let meter_serial = request.meter_serial.clone().unwrap_or_else(|| "unknown".to_string());
    
    // Get meter_id and user_id from database
    // Get meter_id and user_id from database
    let meter_info = sqlx::query!(
        "SELECT id, user_id FROM meters WHERE serial_number = $1",
        meter_serial
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let Some(record) = meter_info {
        let meter_uuid = record.id;
        let user_uuid = record.user_id;

        // Helper to convert Option<f64> to Option<Decimal>
        let to_decimal = |val: Option<f64>| -> Option<Decimal> {
            val.and_then(|v| Decimal::from_f64_retain(v))
        };

        let insert_result = sqlx::query!(
            "INSERT INTO meter_readings (
                id, meter_serial, meter_id, user_id, wallet_address, 
                timestamp, reading_timestamp, kwh_amount,
                energy_generated, energy_consumed, surplus_energy, deficit_energy,
                voltage, current_amps, power_factor, frequency, temperature,
                thd_voltage, thd_current,
                latitude, longitude, battery_level, health_score,
                minted, mint_tx_signature, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $6, $7, 
                       $8::NUMERIC, $9::NUMERIC, $10::NUMERIC, $11::NUMERIC, 
                       $12::NUMERIC, $13::NUMERIC, $14::NUMERIC, $15::NUMERIC, $16::NUMERIC, 
                       $17::NUMERIC, $18::NUMERIC, $19::FLOAT8, $20::FLOAT8, $21::NUMERIC, $22::FLOAT8, 
                       $23, $24, NOW())",
            reading_id,
            meter_serial,
            meter_uuid,
            user_uuid,
            wallet_address,
            request.reading_timestamp,
            // $7 kwh_amount (Decimal)
            request.kwh_amount,
            // Energy (f64 -> Numeric -> Decimal)
            to_decimal(request.energy_generated),
            to_decimal(request.energy_consumed),
            to_decimal(request.surplus_energy),
            to_decimal(request.deficit_energy),
            // Telemetry (f64 -> Numeric -> Decimal)
            to_decimal(request.voltage),
            to_decimal(request.current),
            to_decimal(request.power_factor),
            to_decimal(request.frequency),
            to_decimal(request.temperature),
            // THD (f64 -> Numeric -> Decimal)
            to_decimal(request.thd_voltage),
            to_decimal(request.thd_current),
            // GPS (Float8 is f64, so pass directly)
            request.latitude,
            request.longitude,
            // Battery (f64 -> Numeric -> Decimal)
            to_decimal(request.battery_level),
            // Health
            health_score,
            minted,
            mint_tx_signature
        )
        .execute(&state.db)
        .await;

        match insert_result {
            Ok(_) => info!("✅ Reading {} saved to database", reading_id),
            Err(e) => error!("❌ Failed to save reading to database: {}", e),
        }
    } else {
        warn!("⚠️ Meter info not found for {}, reading not persisted", meter_serial);
    }

    Ok(Json(MeterReadingResponse {
        id: reading_id,
        wallet_address,
        kwh_amount: request.kwh_amount,
        reading_timestamp: request.reading_timestamp,
        submitted_at,
        minted,
        mint_tx_signature,
        message,
    }))
}

/// Health check for meter service
pub async fn meter_health() -> &'static str {
    "Meter stub service is running"
}

// ============================================================================
// Simple Meter Registration (for Simulator)
// ============================================================================

/// Request to register a meter by ID (no auth required, for simulator)
#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterMeterByIdRequest {
    /// Unique meter identifier (serial number)
    pub meter_id: String,
    /// Owner's wallet address
    pub wallet_address: String,
    /// Meter type (e.g., "solar", "consumer")
    pub meter_type: Option<String>,
    /// Location description
    pub location: Option<String>,
    /// GPS latitude
    pub latitude: Option<f64>,
    /// GPS longitude
    pub longitude: Option<f64>,
    /// Zone ID for grid topology
    pub zone_id: Option<i32>,
}

/// Response for meter registration
#[derive(Debug, Serialize)]
pub struct RegisterMeterByIdResponse {
    pub success: bool,
    pub message: String,
    pub meter_id: String,
}

/// Register a meter by ID (simplified, for simulator use)
/// POST /api/v1/simulator/meters/register
pub async fn register_meter_by_id(
    State(state): State<AppState>,
    Json(request): Json<RegisterMeterByIdRequest>,
) -> Result<Json<RegisterMeterByIdResponse>> {
    info!("📝 Register meter by ID: {}", request.meter_id);

    // Check if meter already exists
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM meters WHERE serial_number = $1"
    )
    .bind(&request.meter_id)
    .fetch_one(&state.db)
    .await;

    if let Ok(count) = existing {
        if count > 0 {
            // Check if we have location updates for existing meter
            if request.latitude.is_some() || request.longitude.is_some() {
                // Update meter location
                info!("🔄 Updated location for existing meter {}", request.meter_id);

                // ALSO update meter_registry
                let mut registry_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE meter_registry SET updated_at = NOW()");
                if let Some(loc) = &request.location {
                    registry_builder.push(", location_address = ");
                    registry_builder.push_bind(loc);
                }
                if let Some(zid) = request.zone_id {
                    registry_builder.push(", zone_id = ");
                    registry_builder.push_bind(zid);
                }
                registry_builder.push(" WHERE meter_serial = ");
                registry_builder.push_bind(&request.meter_id);
                let _ = registry_builder.build().execute(&state.db).await;
            }

            info!("✅ Meter {} already registered", request.meter_id);
            return Ok(Json(RegisterMeterByIdResponse {
                success: true,
                message: format!("Meter {} is already registered", request.meter_id),
                meter_id: request.meter_id,
            }));
        }
    }

    // Find or create a system user for simulator meters
    let system_user_id = get_or_create_simulator_user(&state, &request.wallet_address).await?;

    let meter_id = Uuid::new_v4();
    let meter_type = request.meter_type.unwrap_or_else(|| "solar".to_string());
    let location = request.location.unwrap_or_else(|| "Simulator".to_string());

    // Insert into meters table
    let insert_result = sqlx::query(
        "INSERT INTO meters (id, user_id, serial_number, meter_type, location, latitude, longitude, zone_id, is_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, NOW(), NOW())"
    )
    .bind(meter_id)
    .bind(system_user_id)
    .bind(&request.meter_id)
    .bind(&meter_type)
    .bind(&location)
    .bind(request.latitude)
    .bind(request.longitude)
    .bind(request.zone_id)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Meter {} registered successfully", request.meter_id);
            
            // Also insert into meter_registry for FK constraints (with UPSERT)
            let _ = sqlx::query(
                "INSERT INTO meter_registry (id, user_id, meter_serial, meter_type, location_address, meter_key_hash, verification_method, verification_status, zone_id)
                 VALUES ($1, $2, $3, $4, $5, 'simulator_hash', 'auto', 'verified', $6)
                 ON CONFLICT (meter_serial) DO UPDATE SET 
                    zone_id = EXCLUDED.zone_id,
                    location_address = EXCLUDED.location_address,
                    updated_at = NOW()"
            )
            .bind(meter_id)
            .bind(system_user_id)
            .bind(&request.meter_id)
            .bind(&meter_type)
            .bind(&location)
            .bind(request.zone_id)
            .execute(&state.db)
            .await;

            Ok(Json(RegisterMeterByIdResponse {
                success: true,
                message: format!("Meter {} registered and verified", request.meter_id),
                meter_id: request.meter_id,
            }))
        }
        Err(e) => {
            error!("Failed to register meter: {}", e);
            Err(ApiError::Internal(format!("Failed to register meter: {}", e)))
        }
    }
}

/// Get or create a user for simulator meters
async fn get_or_create_simulator_user(state: &AppState, wallet_address: &str) -> Result<Uuid> {
    // Try to find user by wallet address
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users WHERE wallet_address = $1"
    )
    .bind(wallet_address)
    .fetch_optional(&state.db)
    .await;

    if let Ok(Some(user_id)) = existing {
        return Ok(user_id);
    }

    // Create a new simulator user
    let user_id = Uuid::new_v4();
    let email = format!("simulator_{}@gridtokenx.local", &wallet_address[..8.min(wallet_address.len())]);
    
    let insert_result = sqlx::query(
        "INSERT INTO users (id, email, username, password_hash, wallet_address, role, email_verified, created_at)
         VALUES ($1, $2, $3, 'simulator_no_password', $4, 'prosumer', true, NOW())
         ON CONFLICT (email) DO UPDATE SET wallet_address = $4
         RETURNING id"
    )
    .bind(user_id)
    .bind(&email)
    .bind(&email)
    .bind(wallet_address)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Created simulator user for wallet {}", wallet_address);
            Ok(user_id)
        }
        Err(e) => {
            // If insert failed due to conflict, try to fetch again
            if let Ok(Some(uid)) = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM users WHERE wallet_address = $1"
            )
            .bind(wallet_address)
            .fetch_optional(&state.db)
            .await {
                return Ok(uid);
            }
            
            error!("Failed to create simulator user: {}", e);
            Err(ApiError::Internal("Failed to create simulator user".to_string()))
        }
    }
}

/// Check if a meter is registered
pub async fn is_meter_registered(state: &AppState, meter_serial: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM meters WHERE serial_number = $1"
    )
    .bind(meter_serial)
    .fetch_one(&state.db)
    .await
    .map(|c| c > 0)
    .unwrap_or(false)
}
