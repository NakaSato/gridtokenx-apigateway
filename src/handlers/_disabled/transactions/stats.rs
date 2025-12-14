use axum::{extract::State, Json};
use tracing::info;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::handlers::authorization::require_admin;
use crate::models::transaction::TransactionStats;
use crate::AppState;

/// Get transaction statistics
#[utoipa::path(
    get,
    path = "/api/v1/transactions/stats",
    tag = "transactions",
    summary = "Get transaction statistics",
    description = "Retrieve transaction statistics including counts and success rates",
    responses(
        (status = 200, description = "Transaction statistics", body = TransactionStats),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_transaction_stats(
    State(app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<TransactionStats>, ApiError> {
    info!("Getting transaction statistics by user: {:?}", user.sub);

    // Require admin role
    require_admin(&user)?;

    // 1. Count P2P Orders
    let p2p_stats = sqlx::query!(
        r#"
        SELECT 
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'pending') as pending,
            COUNT(*) FILTER (WHERE status = 'partially_filled') as processing,
            COUNT(*) FILTER (WHERE status = 'filled') as confirmed,
            COUNT(*) FILTER (WHERE status = 'cancelled') as failed
        FROM trading_orders
        "#
    )
    .fetch_one(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    // 2. Count Swaps
    let swap_stats = sqlx::query!(
        r#"
        SELECT 
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'completed') as confirmed,
            COUNT(*) FILTER (WHERE status = 'failed') as failed
        FROM swap_transactions
        "#
    )
    .fetch_one(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    // 3. Count Blockchain Txs
    let bc_stats = sqlx::query!(
        r#"
        SELECT 
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'Pending') as pending,
            COUNT(*) FILTER (WHERE status = 'Confirmed' OR status = 'Finalized') as confirmed,
            COUNT(*) FILTER (WHERE status = 'Failed') as failed
        FROM blockchain_transactions
        "#
    )
    .fetch_one(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    // Aggregate
    let total_count =
        p2p_stats.total.unwrap_or(0) + swap_stats.total.unwrap_or(0) + bc_stats.total.unwrap_or(0);

    let pending_count = p2p_stats.pending.unwrap_or(0) + bc_stats.pending.unwrap_or(0);

    let processing_count = p2p_stats.processing.unwrap_or(0);

    let confirmed_count = p2p_stats.confirmed.unwrap_or(0)
        + swap_stats.confirmed.unwrap_or(0)
        + bc_stats.confirmed.unwrap_or(0);

    let failed_count = p2p_stats.failed.unwrap_or(0)
        + swap_stats.failed.unwrap_or(0)
        + bc_stats.failed.unwrap_or(0);

    // Calculate success rate
    let success_rate = if total_count > 0 {
        (confirmed_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(TransactionStats {
        total_count,
        pending_count,
        processing_count,
        submitted_count: 0, // Not separately tracked for now
        confirmed_count,
        failed_count,
        settled_count: confirmed_count, // Treat confirmed as settled for now
        avg_confirmation_time_seconds: None, // Complex to calculate, skip for now
        success_rate,
    }))
}
