use axum::{
    extract::{Path, State},
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{TransactionRetryRequest, TransactionRetryResponse};
use crate::AppState;

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
