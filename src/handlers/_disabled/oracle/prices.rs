use axum::{extract::State, response::Json};
use tracing::{error, info};

use super::types::{CurrentPriceData, PriceSubmissionResponse, SubmitPriceRequest};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Submit price data to oracle
/// POST /api/oracle/prices
#[utoipa::path(
    post,
    path = "/api/oracle/prices",
    tag = "oracle",
    request_body = SubmitPriceRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Price data submitted successfully", body = PriceSubmissionResponse),
        (status = 400, description = "Invalid price or energy type"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_price(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<SubmitPriceRequest>,
) -> Result<Json<PriceSubmissionResponse>> {
    info!(
        "Price submission request from user {}: {} at {} per kWh",
        user.0.sub, payload.energy_type, payload.price_per_kwh
    );

    // Validate price is positive
    if payload.price_per_kwh <= 0.0 {
        return Err(ApiError::BadRequest("Price must be positive".to_string()));
    }

    // Validate energy type
    let valid_types = vec!["solar", "wind", "battery", "grid"];
    if !valid_types.contains(&payload.energy_type.to_lowercase().as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Invalid energy type. Must be one of: {:?}",
            valid_types
        )));
    }

    // Check user role - only admins can submit prices
    let db_user = sqlx::query!(
        "SELECT id, role::text as role FROM users WHERE id = $1",
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch user: {}", e);
        ApiError::Internal("Failed to fetch user data".to_string())
    })?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if db_user.role.as_deref() != Some("admin") && db_user.role.as_deref() != Some("super_admin") {
        return Err(ApiError::Forbidden(
            "Only admins can submit price data".to_string(),
        ));
    }

    let timestamp = payload
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    info!(
        "Price submitted: {} = ${} per kWh at timestamp {}",
        payload.energy_type, payload.price_per_kwh, timestamp
    );

    Ok(Json(PriceSubmissionResponse {
        success: true,
        message: "Price data submitted successfully".to_string(),
        energy_type: payload.energy_type,
        price_per_kwh: payload.price_per_kwh,
        timestamp,
    }))
}

/// Get current price data
/// GET /api/oracle/prices/current
#[utoipa::path(
    get,
    path = "/api/oracle/prices/current",
    tag = "oracle",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current energy prices", body = Vec<CurrentPriceData>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_current_prices(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<CurrentPriceData>>> {
    info!("Fetching current energy prices");

    let prices = vec![
        CurrentPriceData {
            energy_type: "solar".to_string(),
            price_per_kwh: 0.12,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "wind".to_string(),
            price_per_kwh: 0.10,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "battery".to_string(),
            price_per_kwh: 0.15,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
        CurrentPriceData {
            energy_type: "grid".to_string(),
            price_per_kwh: 0.13,
            last_updated: chrono::Utc::now().timestamp(),
            source: "mock".to_string(),
        },
    ];

    Ok(Json(prices))
}
