// Transaction Handlers
// API endpoints for unified blockchain transaction tracking

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use tracing::info;

use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
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

    // TODO: Re-enable when transaction_coordinator is available
    let _ = (app_state, id);
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
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

    // TODO: Re-enable when transaction_coordinator is available
    let _ = (app_state, user, params);
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
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

    // TODO: Re-enable when transaction_coordinator is available
    let _ = app_state;
    Err(ApiError::BadRequest(
        "Transaction coordinator not yet implemented".to_string(),
    ))
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
    /// Convert query parameters to TransactionFilters
    pub fn into_transaction_filters(self, user_id: Option<Uuid>) -> TransactionFilters {
        use chrono::{DateTime, Utc};

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
