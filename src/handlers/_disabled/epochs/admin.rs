use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::Utc;
use uuid::Uuid;

use super::types::*;
use crate::database::schema::types::EpochStatus;
use crate::{error::ApiError, AppState};

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
            id, epoch_number, start_time, end_time, status as "status: EpochStatus",
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
        (Some(total), Some(matched)) if total > 0 => (matched as f64 / total as f64) * 100.0,
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
        status: epoch.status.to_string(),
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
/// Forces an epoch to transition to cleared state and execute order matching
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
        SELECT id, epoch_number, status as "status: EpochStatus"
        FROM market_epochs
        WHERE id = $1
        "#,
        epoch_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound("Epoch not found".into()))?;

    let epoch_status_str = epoch.status.to_string();

    // Check if epoch can be cleared
    if epoch.status != EpochStatus::Active && epoch.status != EpochStatus::Pending {
        return Err(ApiError::BadRequest(format!(
            "Epoch cannot be cleared from status: {}. Must be 'Active' or 'Pending'",
            epoch_status_str
        )));
    }

    // Log::manual clearing action
    tracing::info!(
        "Manual epoch clearing triggered for epoch {} ({}). Reason: {}",
        epoch.epoch_number,
        epoch_id,
        request.reason.as_deref().unwrap_or("No reason provided")
    );

    // Update epoch status to cleared
    sqlx::query(&format!("UPDATE market_epochs SET status = 'cleared'::epoch_status, updated_at = NOW() WHERE id = $1"))
        .bind(epoch_id)
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
            let _ = sqlx::query(&format!("UPDATE market_epochs SET status = '{}'::epoch_status, updated_at = NOW() WHERE id = $1", epoch_status_str))
                .bind(epoch_id)
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
            id, epoch_number, start_time, end_time, status as "status: EpochStatus",
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
