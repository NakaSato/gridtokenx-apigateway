// Market Epochs API Handlers
// Endpoints for querying and managing trading epochs

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use utoipa::ToSchema;

use crate::{error::ApiError, AppState};

/// Current epoch response
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentEpochResponse {
    pub id: String,
    pub epoch_number: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub clearing_price: Option<String>,
    pub total_volume: String,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub time_remaining_seconds: i64,
}

/// Epoch history query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct EpochHistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    20
}

/// Epoch history item
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochHistoryItem {
    pub id: String,
    pub epoch_number: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub clearing_price: Option<String>,
    pub total_volume: String,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Epoch history response
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochHistoryResponse {
    pub epochs: Vec<EpochHistoryItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Epoch statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct EpochStatsResponse {
    pub id: String,
    pub epoch_number: i64,
    pub status: String,
    pub duration_minutes: i64,
    pub total_orders: Option<i64>,
    pub matched_orders: Option<i64>,
    pub match_rate_percent: f64,
    pub total_volume: String,
    pub clearing_price: Option<String>,
    pub unique_traders: i64,
    pub settlements_pending: i64,
    pub settlements_confirmed: i64,
}

/// Manual clearing request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ManualClearingRequest {
    pub reason: Option<String>,
}

/// Manual clearing response
#[derive(Debug, Serialize, ToSchema)]
pub struct ManualClearingResponse {
    pub success: bool,
    pub message: String,
    pub epoch_id: String,
    pub triggered_at: DateTime<Utc>,
}

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
            id, epoch_number, start_time, end_time, status,
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
            status: epoch.status,
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
/// Returns information about the currently active trading epoch
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
            id, epoch_number, start_time, end_time, status,
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
        status: epoch.status,
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
    let mut epochs = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status,
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
        epochs.retain(|e| &e.status == status_filter);
    }

    // Get total count before pagination
    let total = epochs.len() as i64;

    // Apply pagination
    let epochs: Vec<_> = epochs
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    // Get total count (moved after the epochs query to fix unused variable)

    let epoch_items: Vec<EpochHistoryItem> = epochs
        .into_iter()
        .map(|e| EpochHistoryItem {
            id: e.id.to_string(),
            epoch_number: e.epoch_number,
            start_time: e.start_time,
            end_time: e.end_time,
            status: e.status,
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

/// Get detailed statistics for a specific epoch
/// 
/// Returns comprehensive statistics including match rates, unique traders, and settlements
#[utoipa::path(
    get,
    path = "/api/admin/epochs/{epoch_id}/stats",
    tag = "Market Epochs",
    params(
        ("epoch_id" = Uuid, Path, description = "Epoch ID")
    ),
    responses(
        (status = 200, description = "Epoch statistics retrieved", body = EpochStatsResponse),
        (status = 404, description = "Epoch not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_epoch_stats(
    State(state): State<AppState>,
    Path(epoch_id): Path<Uuid>,
) -> Result<Json<EpochStatsResponse>, ApiError> {
    // Get epoch details
    let epoch = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status,
            clearing_price::text, total_volume::text,
            total_orders, matched_orders
        FROM market_epochs
        WHERE id = $1
        "#,
        epoch_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound("Epoch not found".into()))?;

    // Calculate duration
    let duration_minutes = (epoch.end_time - epoch.start_time).num_minutes();

    // Calculate match rate
    let total_orders = epoch.total_orders;
    let matched_orders = epoch.matched_orders;
    let match_rate = match (total_orders, matched_orders) {
        (Some(total), Some(matched)) if total > 0 => {
            (matched as f64 / total as f64) * 100.0
        }
        _ => 0.0,
    };

    // Get unique traders count
    let unique_traders = sqlx::query_scalar!(
        r#"
        SELECT COUNT(DISTINCT user_id) as count
        FROM trading_orders
        WHERE epoch_id = $1
        "#,
        epoch_id
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?
    .unwrap_or(0);

    // Get settlement counts
    let settlements = sqlx::query!(
        r#"
        SELECT 
            COUNT(*) FILTER (WHERE status = 'pending') as pending,
            COUNT(*) FILTER (WHERE status = 'Confirmed') as confirmed
        FROM settlements
        WHERE epoch_id = $1
        "#,
        epoch_id
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok(Json(EpochStatsResponse {
        id: epoch.id.to_string(),
        epoch_number: epoch.epoch_number,
        status: epoch.status,
        duration_minutes,
        total_orders,
        matched_orders,
        match_rate_percent: match_rate,
        total_volume: epoch.total_volume.unwrap_or_else(|| "0".to_string()),
        clearing_price: epoch.clearing_price,
        unique_traders,
        settlements_pending: settlements.pending.unwrap_or(0),
        settlements_confirmed: settlements.confirmed.unwrap_or(0),
    }))
}

/// Manually trigger epoch clearing (Admin only)
/// 
/// Forces an epoch to transition to the cleared state and execute order matching
#[utoipa::path(
    post,
    path = "/api/admin/epochs/{epoch_id}/trigger",
    tag = "Market Epochs",
    params(
        ("epoch_id" = Uuid, Path, description = "Epoch ID to trigger clearing")
    ),
    request_body = ManualClearingRequest,
    responses(
        (status = 200, description = "Epoch clearing triggered successfully", body = ManualClearingResponse),
        (status = 404, description = "Epoch not found"),
        (status = 400, description = "Epoch cannot be cleared (invalid state)"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn trigger_manual_clearing(
    State(state): State<AppState>,
    Path(epoch_id): Path<Uuid>,
    Json(request): Json<ManualClearingRequest>,
) -> Result<Json<ManualClearingResponse>, ApiError> {
    // Verify epoch exists and is in a valid state for clearing
    let epoch = sqlx::query!(
        r#"
        SELECT id, epoch_number, status
        FROM market_epochs
        WHERE id = $1
        "#,
        epoch_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound("Epoch not found".into()))?;

    // Check if epoch can be cleared
    if epoch.status != "Active" && epoch.status != "Pending" {
        return Err(ApiError::BadRequest(format!(
            "Epoch cannot be cleared from status: {}. Must be 'Active' or 'Pending'",
            epoch.status
        )));
    }

    // Log the manual clearing action
    tracing::info!(
        "Manual epoch clearing triggered for epoch {} ({}). Reason: {}",
        epoch.epoch_number,
        epoch_id,
        request.reason.as_deref().unwrap_or("No reason provided")
    );

    // Update epoch status to cleared
    sqlx::query!(
        "UPDATE market_epochs SET status = 'Cleared', updated_at = NOW() WHERE id = $1",
        epoch_id
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Trigger matching cycle
    match state.market_clearing_engine.execute_matching_cycle().await {
        Ok(trades_count) => {
            tracing::info!(
                "Manual clearing complete for epoch {}: {} trades matched",
                epoch.epoch_number,
                trades_count
            );

            Ok(Json(ManualClearingResponse {
                success: true,
                message: format!(
                    "Epoch clearing triggered successfully. {} trades matched.",
                    trades_count
                ),
                epoch_id: epoch_id.to_string(),
                triggered_at: Utc::now(),
            }))
        }
        Err(e) => {
            tracing::error!(
                "Failed to execute matching for epoch {}: {}",
                epoch.epoch_number,
                e
            );

            // Revert status on failure
            let _ = sqlx::query!(
                "UPDATE market_epochs SET status = $1, updated_at = NOW() WHERE id = $2",
                epoch.status,
                epoch_id
            )
            .execute(&state.db)
            .await;

            Err(ApiError::Internal(format!(
                "Failed to execute order matching: {}",
                e
            )))
        }
    }
}

/// Get list of all epochs (Admin only)
/// 
/// Returns a complete list of all epochs with pagination
#[utoipa::path(
    get,
    path = "/api/admin/epochs",
    tag = "Market Epochs",
    params(
        ("limit" = Option<i64>, Query, description = "Number of epochs (default: 50, max: 200)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset (default: 0)")
    ),
    responses(
        (status = 200, description = "Epochs list retrieved", body = EpochHistoryResponse),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_all_epochs(
    State(state): State<AppState>,
    Query(params): Query<EpochHistoryQuery>,
) -> Result<Json<EpochHistoryResponse>, ApiError> {
    // Validate and cap limit (higher for admin)
    let limit = params.limit.min(200).max(1);
    let offset = params.offset.max(0);

    let epochs = sqlx::query!(
        r#"
        SELECT 
            id, epoch_number, start_time, end_time, status,
            clearing_price::text, total_volume::text,
            total_orders, matched_orders, created_at
        FROM market_epochs
        ORDER BY epoch_number DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let total = sqlx::query_scalar!("SELECT COUNT(*) FROM market_epochs")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::Database)?
        .unwrap_or(0);

    let epoch_items: Vec<EpochHistoryItem> = epochs
        .into_iter()
        .map(|e| EpochHistoryItem {
            id: e.id.to_string(),
            epoch_number: e.epoch_number,
            start_time: e.start_time,
            end_time: e.end_time,
            status: e.status,
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
