use axum::{
    extract::{Query, State},
    Json,
};
use tracing::info;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::handlers::authorization::require_admin;
use crate::models::transaction::TransactionResponse;
use crate::AppState;

use super::types::TransactionQueryParams;

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

    // Require admin role
    require_admin(&user)?;

    // TODO: Re-enable when transaction_coordinator is available
    let _ = (app_state, params);
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
}
