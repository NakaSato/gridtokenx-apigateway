use axum::{
    extract::{State, Path},
    Json,
};
use tracing::info;
use uuid::Uuid;
use crate::auth::middleware::AuthenticatedUser;
use crate::AppState;
use super::super::types::{
    MeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    VerifyMeterRequest, UpdateMeterStatusRequest,
};

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
            .map_err(|e| tracing::error!("Failed to sync meter_registry: {}", e));

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
    Path(serial): Path<String>,
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
