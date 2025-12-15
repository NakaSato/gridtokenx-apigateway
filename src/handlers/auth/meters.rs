//! Meters Handlers Module
//!
//! Meter management handlers: registration, verification, readings, etc.

use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use super::types::{
    MeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    VerifyMeterRequest, MeterFilterParams, UpdateMeterStatusRequest,
    CreateReadingRequest, CreateReadingResponse,
};

/// Get user's registered meters from database
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
        // Query meters from database
        let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>)>(
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address
             FROM meters m
             JOIN users u ON m.user_id = u.id
             WHERE m.user_id = $1"
        )
        .bind(claims.sub)
        .fetch_all(&state.db)
        .await;

        if let Ok(meters) = meters_result {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
                }
            }).collect();
            
            info!("✅ Returning {} meters from database", responses.len());
            return Json(responses);
        }
    }

    Json(vec![])
}

/// Get all registered meters (for simulator)
pub async fn get_registered_meters(
    State(state): State<AppState>,
) -> Json<Vec<MeterResponse>> {
    info!("📊 Get all registered meters");
    
    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>)>(
        "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address
         FROM meters m
         JOIN users u ON m.user_id = u.id
         WHERE m.is_verified = true"
    )
    .fetch_all(&state.db)
    .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
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

/// Register a new meter to user account (Step 5: Add serial_id to account)
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

    // Insert meter into database
    let insert_result = sqlx::query(
        "INSERT INTO meters (id, user_id, serial_number, meter_type, location, is_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, false, NOW(), NOW())"
    )
    .bind(meter_id)
    .bind(user_id)
    .bind(&request.serial_number)
    .bind(&meter_type)
    .bind(&location)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => {
            info!("✅ Meter {} registered for user {}", request.serial_number, user_id);
            
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
                    is_verified: false,
                    wallet_address: wallet,
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

/// Verify a meter (mark as verified after smartmeter confirms)
/// Requires meter owner to have verified email first
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
/// GET /api/v1/meters?status=verified
pub async fn get_registered_meters_filtered(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MeterFilterParams>,
) -> Json<Vec<MeterResponse>> {
    info!("📊 Get meters with filter: {:?}", params.status);
    
    let query = match params.status.as_deref() {
        Some("verified") | Some("active") => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = true"
        }
        Some("pending") => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address
             FROM meters m JOIN users u ON m.user_id = u.id WHERE m.is_verified = false"
        }
        _ => {
            "SELECT m.id, m.serial_number, m.meter_type, m.location, m.is_verified, u.wallet_address
             FROM meters m JOIN users u ON m.user_id = u.id"
        }
    };

    let meters_result = sqlx::query_as::<_, (Uuid, String, String, String, bool, Option<String>)>(query)
        .fetch_all(&state.db)
        .await;

    match meters_result {
        Ok(meters) => {
            let responses: Vec<MeterResponse> = meters.iter().map(|(id, serial, mtype, loc, verified, wallet)| {
                MeterResponse {
                    id: *id,
                    serial_number: serial.clone(),
                    meter_type: mtype.clone(),
                    location: loc.clone(),
                    is_verified: *verified,
                    wallet_address: wallet.clone().unwrap_or_default(),
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
/// PATCH /api/v1/meters/{serial}
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
/// POST /api/v1/meters/{serial}/readings
pub async fn create_reading(
    State(state): State<AppState>,
    axum::extract::Path(serial): axum::extract::Path<String>,
    _headers: HeaderMap,
    Json(request): Json<CreateReadingRequest>,
) -> Json<CreateReadingResponse> {
    info!("📊 Create reading for meter {}: {} kWh", serial, request.kwh);

    // Get wallet address from meter or request
    let wallet_address = if let Some(addr) = request.wallet_address.clone() {
        addr
    } else {
        // Try to get wallet from meter's user
        let wallet_result = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT u.wallet_address FROM meters m JOIN users u ON m.user_id = u.id WHERE m.serial_number = $1"
        )
        .bind(&serial)
        .fetch_optional(&state.db)
        .await;

        match wallet_result {
            Ok(Some((Some(w),))) => w,
            _ => {
                return Json(CreateReadingResponse {
                    id: Uuid::new_v4(),
                    serial_number: serial,
                    kwh: request.kwh,
                    timestamp: request.timestamp.unwrap_or_else(chrono::Utc::now),
                    minted: false,
                    tx_signature: None,
                    message: "Wallet address required".to_string(),
                });
            }
        }
    };

    let reading_id = Uuid::new_v4();
    let timestamp = request.timestamp.unwrap_or_else(chrono::Utc::now);

    // Try to mint tokens
    let mut minted = false;
    let mut tx_signature: Option<String> = None;
    let mut message = "Reading recorded".to_string();

    if request.kwh > 0.0 {
        if let Ok(authority) = state.wallet_service.get_authority_keypair().await {
            if let (Ok(mint), Ok(wallet)) = (
                crate::services::BlockchainService::parse_pubkey(&state.config.energy_token_mint),
                crate::services::BlockchainService::parse_pubkey(&wallet_address),
            ) {
                if let Ok(token_account) = state.blockchain_service.ensure_token_account_exists(&authority, &wallet, &mint).await {
                    if let Ok(sig) = state.blockchain_service.mint_energy_tokens(&authority, &token_account, &wallet, &mint, request.kwh).await {
                        minted = true;
                        tx_signature = Some(sig.to_string());
                        message = format!("{} kWh minted successfully", request.kwh);
                        info!("🎉 Minted {} kWh for meter {}", request.kwh, serial);
                    }
                }
            }
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
