//! Transaction Creation Handler
//!
//! API endpoint for creating and submitting blockchain transactions.

use axum::{extract::State, Json};
use tracing::{debug, info};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{CreateTransactionRequest, TransactionResponse};
use crate::AppState;

/// Create and submit a blockchain transaction
///
/// This endpoint accepts a transaction request, validates it, creates a record
/// in the database, and queues it for blockchain submission.
#[utoipa::path(
    post,
    path = "/api/v1/transactions",
    tag = "transactions",
    summary = "Create and submit a blockchain transaction",
    description = "Create a new transaction, validate it, and submit it to the blockchain",
    request_body(
        content = CreateTransactionRequest,
        description = "Transaction creation request",
        content_type = "application/json"
    ),
    responses(
        (status = 202, description = "Transaction accepted for processing", body = TransactionResponse),
        (status = 400, description = "Invalid transaction request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_transaction(
    State(_app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<CreateTransactionRequest>,
) -> Result<Json<TransactionResponse>, ApiError> {
    info!(
        "Creating transaction for user: {:?}, type: {:?}",
        user.sub, request.transaction_type
    );

    // TODO: Implement transaction creation when transaction_coordinator service is added
    debug!("Transaction request received: {:?}", request);
    
    Err(ApiError::BadRequest(
        "Transaction creation is not yet implemented. Use specific trading or swap endpoints.".to_string(),
    ))
}
