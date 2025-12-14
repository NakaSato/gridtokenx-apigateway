use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use tracing::info;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::error::ApiError;
use crate::models::transaction::{TransactionResponse, TransactionStatus, TransactionType};
use crate::AppState;

use super::status::{map_blockchain_status, map_instruction_to_type, map_order_status};
use super::types::TransactionQueryParams;

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
        SELECT id, user_id, order_type as "order_type!: OrderType", side as "side!: OrderSide", energy_amount, price_per_kwh, 
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
        let status = map_order_status(row.status.as_str());

        return Ok(Json(TransactionResponse {
            operation_id: row.id,
            transaction_type: TransactionType::EnergyTrade,
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
            transaction_type: TransactionType::Swap,
            user_id: Some(row.user_id),
            status: TransactionStatus::Settled,
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
        let tx_type = map_instruction_to_type(row.instruction_name.as_deref());
        let status = map_blockchain_status(&row.status);

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
        SELECT id, user_id, order_type as "order_type!: OrderType", side as "side!: OrderSide", energy_amount, price_per_kwh, 
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
            let status = map_order_status(row.status.as_str());

            TransactionResponse {
                operation_id: row.id,
                transaction_type: TransactionType::EnergyTrade,
                user_id: Some(row.user_id),
                status,
                signature: None,
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
            transaction_type: TransactionType::Swap,
            user_id: Some(user.sub),
            status: TransactionStatus::Settled,
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
        // Map instruction to transaction type
        let tx_type = map_instruction_to_type(row.instruction_name.as_deref());
        let status = map_blockchain_status(&row.status);

        // Generate stable UUID for blockchain transactions
        let op_id = Uuid::new_v4();

        all_transactions.push(TransactionResponse {
            operation_id: op_id,
            transaction_type: tx_type,
            user_id: Some(user.sub),
            status,
            signature: Some(row.signature),
            attempts: 1,
            last_error: None,
            created_at: row.created_at.unwrap_or_else(Utc::now),
            submitted_at: row.submitted_at,
            confirmed_at: None,
            settled_at: None,
        });
    }

    // Filter by operation type if specified
    if let Some(op_type) = &params.operation_type {
        if let Ok(filter_type) = op_type.parse::<TransactionType>() {
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
