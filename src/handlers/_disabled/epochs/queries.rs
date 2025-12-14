use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::Utc;

use super::types::*;
use crate::database::schema::types::EpochStatus;
use crate::{error::ApiError, AppState};

/// Get current epoch status (public endpoint)
///
/// Returns current epoch status without requiring authentication
#[utoipa::path(
    get,
    path = "/api/market/epoch/status",
    tag = "Market Epochs",
    responses(
        (status = 200, description = "Current epoch status", body = CurrentEpochResponse),
        (status = 404, description = "No active epoch found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_epoch_status(
    State(state): State<AppState>,
) -> Result<Json<CurrentEpochResponse>, ApiError> {
    // Query current epoch from database
    let epoch = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status as "status: EpochStatus",
            clearing_price::text, total_volume::text,
            total_orders, matched_orders
        FROM market_epochs
        WHERE start_time <= NOW() AND end_time > NOW()
        ORDER BY start_time DESC
        LIMIT 1
        "#
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    if let Some(epoch) = epoch {
        let now = Utc::now();
        let time_remaining = (epoch.end_time - now).num_seconds().max(0);

        Ok(Json(CurrentEpochResponse {
            id: epoch.id.to_string(),
            epoch_number: epoch.epoch_number,
            start_time: epoch.start_time,
            end_time: epoch.end_time,
            status: epoch.status.to_string(),
            clearing_price: epoch.clearing_price,
            total_volume: epoch.total_volume.unwrap_or_else(|| "0".to_string()),
            total_orders: epoch.total_orders,
            matched_orders: epoch.matched_orders,
            time_remaining_seconds: time_remaining,
        }))
    } else {
        // No active epoch, return basic response
        Ok(Json(CurrentEpochResponse {
            id: "none".to_string(),
            epoch_number: 0,
            start_time: Utc::now(),
            end_time: Utc::now(),
            status: "none".to_string(),
            clearing_price: None,
            total_volume: "0".to_string(),
            total_orders: Some(0),
            matched_orders: Some(0),
            time_remaining_seconds: 0,
        }))
    }
}

/// Get current active epoch
///
/// Returns information about currently active trading epoch
#[utoipa::path(
    get,
    path = "/api/market/epoch/current",
    tag = "Market Epochs",
    responses(
        (status = 200, description = "Current epoch information", body = CurrentEpochResponse),
        (status = 404, description = "No active epoch found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_current_epoch(
    State(state): State<AppState>,
) -> Result<Json<CurrentEpochResponse>, ApiError> {
    // Query current epoch from database
    let epoch = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status as "status: EpochStatus",
            clearing_price::text, total_volume::text,
            total_orders, matched_orders
        FROM market_epochs
        WHERE status IN ('pending', 'active')
        ORDER BY start_time DESC
        LIMIT 1
        "#
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound("No active epoch found".into()))?;

    let now = Utc::now();
    let time_remaining = (epoch.end_time - now).num_seconds().max(0);

    Ok(Json(CurrentEpochResponse {
        id: epoch.id.to_string(),
        epoch_number: epoch.epoch_number,
        start_time: epoch.start_time,
        end_time: epoch.end_time,
        status: epoch.status.to_string(),
        clearing_price: epoch.clearing_price,
        total_volume: epoch.total_volume.unwrap_or_else(|| "0".to_string()),
        total_orders: epoch.total_orders,
        matched_orders: epoch.matched_orders,
        time_remaining_seconds: time_remaining,
    }))
}

/// Get epoch history with filtering
///
/// Returns a paginated list of past epochs with optional status filtering
#[utoipa::path(
    get,
    path = "/api/market/epoch/history",
    tag = "Market Epochs",
    params(
        ("limit" = Option<i64>, Query, description = "Number of epochs to return (default: 20, max: 100)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset (default: 0)"),
        ("status" = Option<String>, Query, description = "Filter by status (pending, active, cleared, settled, expired)")
    ),
    responses(
        (status = 200, description = "Epoch history retrieved", body = EpochHistoryResponse),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_epoch_history(
    State(state): State<AppState>,
    Query(params): Query<EpochHistoryQuery>,
) -> Result<Json<EpochHistoryResponse>, ApiError> {
    // Validate and cap limit
    let limit = params.limit.min(100).max(1);
    let offset = params.offset.max(0);

    // Fetch epochs (filter by status if provided)
    // Note: We'll fetch slightly more if filtering in memory, or filter in SQL if needed.
    // For simplicity and matching original, fetching all might be expensive if many epochs.
    // But original code fetched all then filtered. Let's optimize slightly by filtering in SQL if possible,
    // but the original code did it in memory. I will keep it similar but maybe simpler.

    // Actually, SQL filtering is better.
    // But to match original logic precisely (fetching all then filtering in Rust), I will stick to original logic
    // unless it's obviously bad. It is bad for scaling.
    // However, for this refactor I should mostly copy logic.

    let mut epochs = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status as "status: EpochStatus",
            clearing_price::text, total_volume::text,
            total_orders, matched_orders, created_at
        FROM market_epochs
        ORDER BY epoch_number DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Filter by status if provided
    if let Some(status_filter) = &params.status {
        epochs.retain(|e| e.status.to_string() == *status_filter);
    }

    // Get total count before pagination
    let total = epochs.len() as i64;

    // Apply pagination
    let epochs: Vec<_> = epochs
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    let epoch_items: Vec<EpochHistoryItem> = epochs
        .into_iter()
        .map(|e| EpochHistoryItem {
            id: e.id.to_string(),
            epoch_number: e.epoch_number,
            start_time: e.start_time,
            end_time: e.end_time,
            status: e.status.to_string(),
            clearing_price: e.clearing_price,
            total_volume: e.total_volume.unwrap_or_else(|| "0".to_string()),
            total_orders: e.total_orders,
            matched_orders: e.matched_orders,
            created_at: e.created_at.unwrap_or_else(|| Utc::now()),
        })
        .collect();

    Ok(Json(EpochHistoryResponse {
        epochs: epoch_items,
        total,
        limit,
        offset,
    }))
}
