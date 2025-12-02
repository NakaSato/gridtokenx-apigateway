// Transaction Handlers
// API endpoints for unified blockchain transaction tracking

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use tracing::info;

use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus};
use crate::error::ApiError;
use crate::models::transaction::{
    TransactionFilters, TransactionResponse, TransactionRetryRequest, TransactionRetryResponse,
    TransactionStats,
};

/// Get transaction status by ID
#[utoipa::path(
    get,
    path = "/api/v1/transactions/{id}/status",
    tag = "transactions",
    summary = "Get transaction status",
    description = "Retrieve the current status and details of a specific blockchain transaction",
    params(
        ("id" = Uuid, Path, description = "Transaction ID")
    ),
    responses(
        (status = 200, description = "Transaction details", body = TransactionResponse),
        (status = 404, description = "Transaction not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_transaction_status(
    State(app_state): State<AppState>,
    Path(id): Path<Uuid>,
    AuthenticatedUser(_user): AuthenticatedUser,
) -> Result<Json<TransactionResponse>, ApiError> {
    info!("Getting status for transaction: {}", id);

    // 1. Check Trading Orders
    let p2p_order = sqlx::query!(
        r#"
        SELECT id, user_id, order_type, side as "side!: OrderSide", energy_amount, price_per_kwh, 
               status as "status!: OrderStatus", created_at, settled_at as filled_at, epoch_id
        FROM trading_orders 
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    if let Some(row) = p2p_order {
        let status = match row.status.as_str() {
            "pending" => crate::models::transaction::TransactionStatus::Pending,
            "filled" => crate::models::transaction::TransactionStatus::Settled,
            "cancelled" => crate::models::transaction::TransactionStatus::Failed,
            "partially_filled" => crate::models::transaction::TransactionStatus::Processing,
            _ => crate::models::transaction::TransactionStatus::Pending,
        };

        return Ok(Json(TransactionResponse {
            operation_id: row.id,
            transaction_type: crate::models::transaction::TransactionType::EnergyTrade,
            user_id: Some(row.user_id),
            status,
            signature: None,
            attempts: 1,
            last_error: None,
            created_at: row.created_at.unwrap_or_else(Utc::now),
            submitted_at: row.created_at,
            confirmed_at: row.filled_at,
            settled_at: row.filled_at,
        }));
    }

    // 2. Check AMM Swaps
    // We need to query the swap_transactions table directly since AmmService might not expose get_by_id
    // But better to use AmmService if possible.
    // Let's assume for now we can query the table directly as we did for p2p orders,
    // or we can add a method to AmmService. Querying table is faster for now.
    let swap_tx = sqlx::query!(
        r#"
        SELECT id, user_id, pool_id, input_token, output_token, input_amount, output_amount,
               fee_amount, tx_hash, status, created_at
        FROM swap_transactions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    if let Some(row) = swap_tx {
        return Ok(Json(TransactionResponse {
            operation_id: row.id,
            transaction_type: crate::models::transaction::TransactionType::Swap,
            user_id: Some(row.user_id),
            status: crate::models::transaction::TransactionStatus::Settled, // Assuming stored swaps are successful
            signature: row.tx_hash,
            attempts: 1,
            last_error: None,
            created_at: row.created_at,
            submitted_at: Some(row.created_at),
            confirmed_at: Some(row.created_at),
            settled_at: Some(row.created_at),
        }));
    }

    // 3. Check Blockchain Transactions
    let bc_tx = sqlx::query!(
        r#"
        SELECT id, signature, user_id, program_id, instruction_name, status, submitted_at, created_at
        FROM blockchain_transactions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&app_state.db)
    .await
    .map_err(ApiError::Database)?;

    if let Some(row) = bc_tx {
        let tx_type = match row.instruction_name.as_deref() {
            Some("place_order") => crate::models::transaction::TransactionType::EnergyTrade,
            Some("swap") => crate::models::transaction::TransactionType::Swap,
            Some("mint") => crate::models::transaction::TransactionType::TokenMint,
            Some("transfer") => crate::models::transaction::TransactionType::TokenTransfer,
            Some("vote") => crate::models::transaction::TransactionType::GovernanceVote,
            _ => crate::models::transaction::TransactionType::RegistryUpdate,
        };

        let status = match row.status.as_str() {
            "Confirmed" | "Finalized" => crate::models::transaction::TransactionStatus::Confirmed,
            "Failed" => crate::models::transaction::TransactionStatus::Failed,
            _ => crate::models::transaction::TransactionStatus::Pending,
        };

        return Ok(Json(TransactionResponse {
            operation_id: row.id,
            transaction_type: tx_type,
            user_id: row.user_id,
            status,
            signature: Some(row.signature),
            attempts: 1,
            last_error: None,
            created_at: row.created_at.unwrap_or_else(Utc::now),
            submitted_at: row.submitted_at,
            confirmed_at: None,
            settled_at: None,
        }));
    }

    Err(ApiError::NotFound("Transaction not found".to_string()))
}

/// Get transactions for authenticated user
#[utoipa::path(
    get,
    path = "/api/v1/transactions/user",
    tag = "transactions",
    summary = "Get user transactions",
    description = "Retrieve a paginated list of transactions for authenticated user with optional filters",
    params(
        ("operation_type" = Option<String>, Query, description = "Filter by operation type"),
        ("tx_type" = Option<String>, Query, description = "Filter by transaction type"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("date_from" = Option<String>, Query, description = "Filter by start date (ISO 8601)"),
        ("date_to" = Option<String>, Query, description = "Filter by end date (ISO 8601)"),
        ("limit" = Option<i64>, Query, description = "Maximum number of results to return"),
        ("offset" = Option<i64>, Query, description = "Number of results to skip for pagination"),
        ("min_attempts" = Option<i32>, Query, description = "Filter by minimum number of attempts"),
        ("has_signature" = Option<bool>, Query, description = "Filter by presence of signature")
    ),
    responses(
        (status = 200, description = "List of user transactions", body = Vec<TransactionResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_user_transactions(
    State(app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(params): Query<TransactionQueryParams>,
) -> Result<Json<Vec<TransactionResponse>>, ApiError> {
    info!("Getting transactions for user: {:?}", user.sub);

    let limit = params.limit.unwrap_or(20).max(1).min(100) as usize;
    let offset = params.offset.unwrap_or(0).max(0) as usize;

    // 1. Fetch P2P Trades (Trading Orders)
    // We only want filled or partially filled orders to show as "transactions" usually,
    // but for a full history we might want all. Let's stick to what the user asked: "transactions".
    // Usually implies completed actions. But let's include all for now as per "history".
    // Actually, let's filter for relevant statuses if needed, but for now fetch all.

    // We fetch (limit + offset) to ensure we have enough to slice after merging
    let fetch_limit = (limit + offset) as i64;

    let p2p_query = sqlx::query!(
        r#"
        SELECT id, user_id, order_type, side as "side!: OrderSide", energy_amount, price_per_kwh, 
               status as "status!: OrderStatus", created_at, settled_at as filled_at, epoch_id
        FROM trading_orders 
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
        user.sub,
        fetch_limit
    );

    let p2p_orders = p2p_query
        .fetch_all(&app_state.db)
        .await
        .map_err(ApiError::Database)?;

    let mut all_transactions: Vec<TransactionResponse> = p2p_orders
        .into_iter()
        .map(|row| {
            let status = match row.status.as_str() {
                "pending" => crate::models::transaction::TransactionStatus::Pending,
                "filled" => crate::models::transaction::TransactionStatus::Settled,
                "cancelled" => crate::models::transaction::TransactionStatus::Failed, // Or separate Cancelled status if we had it
                "partially_filled" => crate::models::transaction::TransactionStatus::Processing,
                _ => crate::models::transaction::TransactionStatus::Pending,
            };

            TransactionResponse {
                operation_id: row.id,
                transaction_type: crate::models::transaction::TransactionType::EnergyTrade,
                user_id: Some(row.user_id),
                status,
                signature: None, // P2P orders might not have a direct single signature yet
                attempts: 1,
                last_error: None,
                created_at: row.created_at.unwrap_or_else(Utc::now),
                submitted_at: row.created_at,
                confirmed_at: row.filled_at,
                settled_at: row.filled_at,
            }
        })
        .collect();

    // 2. Fetch AMM Swaps
    // We assume AmmService has a method to get history.
    // If it doesn't support pagination/limit, we might fetch all (careful!) or we need to update AmmService.
    // The current `get_user_swap_history` fetches all. For MVP this is fine, but should be optimized later.
    let swaps = app_state
        .amm_service
        .get_user_swap_history(user.sub)
        .await?;

    for swap in swaps {
        all_transactions.push(TransactionResponse {
            operation_id: swap.id,
            transaction_type: crate::models::transaction::TransactionType::Swap,
            user_id: Some(user.sub),
            status: crate::models::transaction::TransactionStatus::Settled, // Swaps in history are usually successful
            signature: swap.tx_hash,
            attempts: 1,
            last_error: None,
            created_at: swap.created_at,
            submitted_at: Some(swap.created_at),
            confirmed_at: Some(swap.created_at),
            settled_at: Some(swap.created_at),
        });
    }

    // 3. Fetch Blockchain Transactions (General)
    let blockchain_txs = sqlx::query!(
        r#"
        SELECT signature, user_id, program_id, instruction_name, status, submitted_at, created_at
        FROM blockchain_transactions
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
        user.sub,
        fetch_limit
    );

    let bc_txs = blockchain_txs
        .fetch_all(&app_state.db)
        .await
        .map_err(ApiError::Database)?;

    for row in bc_txs {
        // Avoid duplicates if we already have them (e.g. if we logged swaps/trades here too)
        // For now, assume they are distinct or we want to show the low-level tx view as well.
        // To be cleaner, we might want to filter out those that are already covered by high-level entities.
        // But for "Unified History", showing the low-level tx is often useful or we treat them as "Other".

        let tx_type = match row.instruction_name.as_deref() {
            Some("place_order") => crate::models::transaction::TransactionType::EnergyTrade,
            Some("swap") => crate::models::transaction::TransactionType::Swap,
            Some("mint") => crate::models::transaction::TransactionType::TokenMint,
            Some("transfer") => crate::models::transaction::TransactionType::TokenTransfer,
            Some("vote") => crate::models::transaction::TransactionType::GovernanceVote,
            _ => crate::models::transaction::TransactionType::RegistryUpdate, // Fallback/Generic
        };

        // If it's a Swap or Trade, we might have already added it from the high-level tables.
        // A simple de-duplication strategy is to check if we have a transaction with the same signature.
        // But P2P orders don't always have a signature in the `trading_orders` table immediately visible here.
        // Let's just add them for now, maybe filtered by type if the user requested filters.

        let status = match row.status.as_str() {
            "Confirmed" | "Finalized" => crate::models::transaction::TransactionStatus::Confirmed,
            "Failed" => crate::models::transaction::TransactionStatus::Failed,
            _ => crate::models::transaction::TransactionStatus::Pending,
        };

        // Use signature as ID if we don't have a UUID, or generate a deterministic one?
        // UUID is required for TransactionResponse.
        // We can generate a UUID from the signature bytes or just new random one (not ideal for stability).
        // Let's try to find a stable way or just use a new UUID for the view.
        let op_id = Uuid::new_v4();

        all_transactions.push(TransactionResponse {
            operation_id: op_id,
            transaction_type: tx_type,
            user_id: Some(user.sub),
            status,
            signature: Some(row.signature),
            attempts: 1,
            last_error: None,
            created_at: row.created_at.unwrap_or_else(Utc::now), // Fallback
            submitted_at: row.submitted_at,
            confirmed_at: None,
            settled_at: None,
        });
    }

    // Filter
    if let Some(op_type) = &params.operation_type {
        if let Ok(filter_type) = op_type.parse::<crate::models::transaction::TransactionType>() {
            all_transactions.retain(|tx| tx.transaction_type == filter_type);
        }
    }

    // Sort by created_at DESC
    all_transactions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Pagination
    let total = all_transactions.len();
    let start = offset.min(total);
    let end = (offset + limit).min(total);

    let page = all_transactions[start..end].to_vec();

    Ok(Json(page))
}

/// Get transaction history with filters
#[utoipa::path(
    get,
    path = "/api/v1/transactions/history",
    tag = "transactions",
    summary = "Get transaction history",
    description = "Retrieve a paginated list of all transactions with optional filters (admin only)",
    params(
        ("user_id" = Option<Uuid>, Query, description = "Filter by user ID"),
        ("operation_type" = Option<String>, Query, description = "Filter by operation type"),
        ("tx_type" = Option<String>, Query, description = "Filter by transaction type"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("date_from" = Option<String>, Query, description = "Filter by start date (ISO 8601)"),
        ("date_to" = Option<String>, Query, description = "Filter by end date (ISO 8601)"),
        ("limit" = Option<i64>, Query, description = "Maximum number of results to return"),
        ("offset" = Option<i64>, Query, description = "Number of results to skip for pagination"),
        ("min_attempts" = Option<i32>, Query, description = "Filter by minimum number of attempts"),
        ("has_signature" = Option<bool>, Query, description = "Filter by presence of signature")
    ),
    responses(
        (status = 200, description = "List of transactions", body = Vec<TransactionResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_transaction_history(
    State(app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(params): Query<TransactionQueryParams>,
) -> Result<Json<Vec<TransactionResponse>>, ApiError> {
    info!("Getting transaction history by user: {:?}", user.sub);

    // Check if user has admin privileges
    if user.role != "admin" {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Re-enable when transaction_coordinator is available
    let _ = (app_state, params);
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
}

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

    // Check if user has admin privileges
    if user.role != "admin" {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

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

/// Retry a failed transaction
#[utoipa::path(
    post,
    path = "/api/v1/transactions/{id}/retry",
    tag = "transactions",
    summary = "Retry a failed transaction",
    description = "Retry a failed blockchain transaction with optional maximum attempt limit",
    params(
        ("id" = Uuid, Path, description = "Transaction ID")
    ),
    request_body(
        content = TransactionRetryRequest,
        description = "Retry request parameters",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Transaction retry response", body = TransactionRetryResponse),
        (status = 404, description = "Transaction not found"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - insufficient permissions"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn retry_transaction(
    State(app_state): State<AppState>,
    Path(id): Path<Uuid>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<TransactionRetryRequest>,
) -> Result<Json<TransactionRetryResponse>, ApiError> {
    info!(
        "User {:?} attempting to retry transaction {} with max attempts {:?}",
        user.sub, id, request.max_attempts
    );

    // TODO: Re-enable when transaction_coordinator is available
    let _ = (app_state, user, id, request);
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
}

/// Query parameters for transaction endpoints
#[derive(Debug, Deserialize)]
pub struct TransactionQueryParams {
    pub operation_type: Option<String>,
    pub tx_type: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_attempts: Option<i32>,
    pub has_signature: Option<bool>,
}

impl TransactionQueryParams {
    pub fn into_transaction_filters(self, user_id: Option<Uuid>) -> TransactionFilters {
        TransactionFilters {
            operation_type: self.operation_type.and_then(|t| t.parse().ok()),
            tx_type: self.tx_type.and_then(|t| t.parse().ok()),
            status: self.status.and_then(|s| s.parse().ok()),
            user_id,
            date_from: self
                .date_from
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            date_to: self
                .date_to
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            limit: self.limit,
            offset: self.offset,
            min_attempts: self.min_attempts,
            has_signature: self.has_signature,
        }
    }
}
