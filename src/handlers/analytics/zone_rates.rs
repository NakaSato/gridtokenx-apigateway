use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::AppState;
use crate::error::{ApiError, Result};

/// Zone rate response - derives FromRow for sqlx
#[derive(Debug, Serialize, FromRow)]
pub struct ZoneRateResponse {
    pub id: i32,
    pub from_zone_id: i32,
    pub to_zone_id: i32,
    pub wheeling_charge: Decimal,
    pub loss_factor: Decimal,
    pub description: Option<String>,
    pub is_active: bool,
    pub effective_from: chrono::DateTime<chrono::Utc>,
    pub effective_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// Create zone rate request
#[derive(Debug, Deserialize)]
pub struct CreateZoneRateRequest {
    pub from_zone_id: i32,
    pub to_zone_id: i32,
    pub wheeling_charge: Decimal,
    pub loss_factor: Decimal,
    pub description: Option<String>,
}

/// Full update zone rate request (all fields required for straightforward sqlx binding)
#[derive(Debug, Deserialize)]
pub struct UpdateZoneRateRequest {
    pub wheeling_charge: Decimal,
    pub loss_factor: Decimal,
    pub description: Option<String>,
    pub is_active: bool,
}

/// List all zone rates
pub async fn list_zone_rates(
    State(state): State<AppState>,
) -> Result<Json<Vec<ZoneRateResponse>>> {
    let rates = sqlx::query_as::<_, ZoneRateResponse>(
        r#"
        SELECT 
            id,
            from_zone_id,
            to_zone_id,
            wheeling_charge,
            loss_factor,
            description,
            is_active,
            effective_from,
            effective_until
        FROM zone_rates
        ORDER BY from_zone_id, to_zone_id
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok(Json(rates))
}

/// Get a specific zone rate by zone pair
pub async fn get_zone_rate_by_zones(
    State(state): State<AppState>,
    Path((from_zone, to_zone)): Path<(i32, i32)>,
) -> Result<Json<ZoneRateResponse>> {
    let rate = sqlx::query_as::<_, ZoneRateResponse>(
        r#"
        SELECT 
            id,
            from_zone_id,
            to_zone_id,
            wheeling_charge,
            loss_factor,
            description,
            is_active,
            effective_from,
            effective_until
        FROM zone_rates
        WHERE from_zone_id = $1 AND to_zone_id = $2 AND is_active = TRUE
        ORDER BY effective_from DESC
        LIMIT 1
        "#
    )
    .bind(from_zone)
    .bind(to_zone)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    match rate {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError::NotFound("Zone rate not found".to_string())),
    }
}

/// Create a new zone rate
pub async fn create_zone_rate(
    State(state): State<AppState>,
    Json(req): Json<CreateZoneRateRequest>,
) -> Result<(StatusCode, Json<ZoneRateResponse>)> {
    let rate = sqlx::query_as::<_, ZoneRateResponse>(
        r#"
        INSERT INTO zone_rates (from_zone_id, to_zone_id, wheeling_charge, loss_factor, description)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING 
            id,
            from_zone_id,
            to_zone_id,
            wheeling_charge,
            loss_factor,
            description,
            is_active,
            effective_from,
            effective_until
        "#
    )
    .bind(req.from_zone_id)
    .bind(req.to_zone_id)
    .bind(req.wheeling_charge)
    .bind(req.loss_factor)
    .bind(&req.description)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok((StatusCode::CREATED, Json(rate)))
}

/// Update an existing zone rate
pub async fn update_zone_rate(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateZoneRateRequest>,
) -> Result<Json<ZoneRateResponse>> {
    let rate = sqlx::query_as::<_, ZoneRateResponse>(
        r#"
        UPDATE zone_rates
        SET 
            wheeling_charge = $2,
            loss_factor = $3,
            description = $4,
            is_active = $5,
            updated_at = NOW()
        WHERE id = $1
        RETURNING 
            id,
            from_zone_id,
            to_zone_id,
            wheeling_charge,
            loss_factor,
            description,
            is_active,
            effective_from,
            effective_until
        "#
    )
    .bind(id)
    .bind(req.wheeling_charge)
    .bind(req.loss_factor)
    .bind(&req.description)
    .bind(req.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    match rate {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError::NotFound("Zone rate not found".to_string())),
    }
}

/// Deactivate (soft delete) a zone rate
pub async fn delete_zone_rate(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode> {
    let result = sqlx::query_as::<_, (i32,)>(
        r#"
        UPDATE zone_rates
        SET is_active = FALSE, updated_at = NOW()
        WHERE id = $1
        RETURNING id
        "#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    match result {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(ApiError::NotFound("Zone rate not found".to_string())),
    }
}
