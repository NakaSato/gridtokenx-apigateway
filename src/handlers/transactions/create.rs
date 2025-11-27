// Transaction Creation Handler
// API endpoint for creating and submitting blockchain transactions

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{CreateTransactionRequest, TransactionResponse};

/// Create and submit a blockchain transaction
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
    State(app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<CreateTransactionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    info!("Creating transaction for user: {:?}", user.sub);

    // 1. Validate transaction
    app_state
        .validation_service
        .validate_transaction(&request)
        .await?;

    // 2. Create transaction record
    let transaction = app_state
        .transaction_coordinator
        .create_transaction(user.sub, request)
        .await?;

    // 3. Submit to blockchain asynchronously
    let coordinator = app_state.transaction_coordinator.clone();
    tokio::spawn(async move {
        if let Err(e) = coordinator
            .submit_to_blockchain(transaction.operation_id)
            .await
        {
            error!(
                "Failed to submit transaction {}: {}",
                transaction.operation_id, e
            );
            // Mark transaction as failed in the database
            if let Err(db_err) = coordinator
                .mark_transaction_failed(transaction.operation_id, &e.to_string())
                .await
            {
                error!("Failed to mark transaction as failed: {}", db_err);
            }
        }
    });

    debug!("Transaction created successfully: {:?}", transaction);
    Ok((StatusCode::ACCEPTED, Json(transaction)))
}
