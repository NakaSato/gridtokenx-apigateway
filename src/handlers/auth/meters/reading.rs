use axum::{
    extract::{State, Path, Query},
    http::HeaderMap,
    Json,
};
use tracing::{info, error, warn, debug};
use uuid::Uuid;
use crate::AppState;
use super::super::types::{
    CreateReadingRequest, CreateReadingResponse, CreateReadingParams, 
    CreateBatchReadingRequest, BatchReadingResponse,
};
use crate::services::meter_analyzer::{check_alerts, calculate_health_score};
use rust_decimal::prelude::ToPrimitive;
use serde_json;

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
    Path(serial): Path<String>,
    Query(params): Query<CreateReadingParams>,
    _headers: HeaderMap,
    Json(request): Json<CreateReadingRequest>,
) -> Json<CreateReadingResponse> {
    Json(internal_create_reading(&state, serial, params, request).await)
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
