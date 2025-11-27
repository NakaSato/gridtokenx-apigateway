// Transaction Status Handler
// API endpoint for retrieving transaction status

use axum::{
    Json,
    extract::Path,
};
use tracing::debug;
use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::TransactionResponse;

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
    debug!("Getting status for transaction: {}", id);

    let transaction = app_state
        .transaction_coordinator
        .get_transaction_status(id)
        .await?;

    debug!("Retrieved transaction: {:?}", transaction);
    Ok(Json(transaction))
}
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/create.rs
</file_path>

<edit_description>
Create transaction creation handler
</edit_description>
```rust
// Transaction Creation Handler
// API endpoint for creating and submitting blockchain transactions

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    CreateTransactionRequest, TransactionResponse, TransactionType,
};

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
    info!("Creating transaction: type={:?}, user_id={}", request.transaction_type, user.sub);

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
    let operation_id = transaction.operation_id;

    tokio::spawn(async move {
        info!("Submitting transaction {} to blockchain", operation_id);

        match coordinator.submit_to_blockchain(operation_id).await {
            Ok(()) => {
                info!("Transaction {} submitted successfully", operation_id);
            }
            Err(e) => {
                error!("Failed to submit transaction {}: {}", operation_id, e);
                // Mark transaction as failed in database
                if let Err(db_err) = coordinator.mark_transaction_failed(operation_id, &e.to_string()).await {
                    error!("Failed to mark transaction as failed: {}", db_err);
                }
            }
        }
    });

    debug!("Transaction created successfully: {:?}", transaction);
    Ok((StatusCode::ACCEPTED, Json(transaction)))
}

/// Query parameters for transaction filtering
#[derive(Debug, Deserialize)]
pub struct TransactionQueryParams {
    pub transaction_type: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_attempts: Option<i32>,
    pub has_signature: Option<bool>,
}

/// Get transactions for authenticated user
#[utoipa::path(
    get,
    path = "/api/v1/transactions/user",
    tag = "transactions",
    summary = "Get user transactions",
    description = "Retrieve a paginated list of transactions for authenticated user with optional filters",
    params(
        ("transaction_type" = Option<String>, Query, description = "Filter by transaction type"),
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

    // Convert query parameters to filters
    let filters = params.into_transaction_filters(Some(user.sub));

    let transactions = app_state
        .transaction_coordinator
        .get_user_transactions(user.sub, filters)
        .await?;

    debug!("Retrieved {} transactions for user", transactions.len());
    Ok(Json(transactions))
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
        ("transaction_type" = Option<String>, Query, description = "Filter by transaction type"),
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

    let filters = params.into_transaction_filters(None);
    let transactions = app_state
        .transaction_coordinator
        .get_transactions(filters)
        .await?;

    debug!("Retrieved {} transactions", transactions.len());
    Ok(Json(transactions))
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

    let stats = app_state
        .transaction_coordinator
        .get_transaction_stats()
        .await?;

    debug!("Retrieved transaction stats: {:?}", stats);
    Ok(Json(stats))
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

    // Get transaction details to check permissions
    let transaction = app_state
        .transaction_coordinator
        .get_transaction_status(id)
        .await?;

    // Check if user has permission to retry this transaction
    match transaction.user_id {
        Some(transaction_user_id) => {
            // Users can only retry their own transactions
            if transaction_user_id != user.sub {
                return Err(ApiError::Forbidden(
                    "You can only retry your own transactions".to_string(),
                ));
            }
        }
        None => {
            // If no user_id, only admin can retry
            if user.role != "admin" {
                return Err(ApiError::Forbidden(
                    "Admin access required to retry this transaction".to_string(),
                ));
            }
        }
    }

    // Merge ID from path with the request
    let retry_request = TransactionRetryRequest {
        operation_id: id,
        operation_type: request.operation_type,
        max_attempts: request.max_attempts,
    };

    let result = app_state
        .transaction_coordinator
        .retry_transaction(retry_request)
        .await?;

    info!(
        "Transaction retry completed with success: {}, attempts: {}",
        result.success, result.attempts
    );

    Ok(Json(result))
}

/// Transaction retry request
#[derive(Debug, Deserialize)]
pub struct TransactionRetryRequest {
    pub operation_type: String,
    pub max_attempts: Option<i32>,
}

/// Transaction retry response
#[derive(Debug, Serialize)]
pub struct TransactionRetryResponse {
    pub success: bool,
    pub attempts: i32,
    pub new_signature: Option<String>,
    pub message: String,
}

/// Transaction statistics
#[derive(Debug, Serialize)]
pub struct TransactionStats {
    pub total_count: i64,
    pub pending_count: i64,
    pub processing_count: i64,
    pub submitted_count: i64,
    pub confirmed_count: i64,
    pub failed_count: i64,
    pub settled_count: i64,
    pub avg_confirmation_time_seconds: Option<f64>,
    pub success_rate: f64,
}

impl TransactionQueryParams {
    /// Convert query parameters to TransactionFilters
    pub fn into_transaction_filters(self, user_id: Option<Uuid>) -> crate::models::transaction::TransactionFilters {
        use chrono::{DateTime, Utc};

        crate::models::transaction::TransactionFilters {
            transaction_type: self.transaction_type,
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
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/mod.rs
</file_path>

<edit_description>
Update transactions handlers mod
</edit_description>
```rust
// Transaction Handlers
// API endpoints for unified blockchain transaction tracking

pub mod create;
pub mod status;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use tracing::{debug, info};

use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    TransactionFilters, TransactionResponse, TransactionRetryRequest, TransactionRetryResponse,
    TransactionStats, TransactionType,
};

// Re-export handlers
pub use create::{
    create_transaction, get_user_transactions, get_transaction_history,
    get_transaction_stats, retry_transaction,
    TransactionQueryParams,
};
pub use status::get_transaction_status;
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/create.rs
</file_path>

<edit_description>
Update create handler to export needed types
</edit_description>
```rust
// Transaction Creation Handler
// API endpoint for creating and submitting blockchain transactions

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    CreateTransactionRequest, TransactionResponse, TransactionType,
    TransactionRetryRequest, TransactionRetryResponse, TransactionStats,
};

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
    info!("Creating transaction: type={:?}, user_id={}", request.transaction_type, user.sub);

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
    let operation_id = transaction.operation_id;

    tokio::spawn(async move {
        info!("Submitting transaction {} to blockchain", operation_id);

        match coordinator.submit_to_blockchain(operation_id).await {
            Ok(()) => {
                info!("Transaction {} submitted successfully", operation_id);
            }
            Err(e) => {
                error!("Failed to submit transaction {}: {}", operation_id, e);
                // Mark transaction as failed in database
                if let Err(db_err) = coordinator.mark_transaction_failed(operation_id, &e.to_string()).await {
                    error!("Failed to mark transaction as failed: {}", db_err);
                }
            }
        }
    });

    debug!("Transaction created successfully: {:?}", transaction);
    Ok((StatusCode::ACCEPTED, Json(transaction)))
}

/// Query parameters for transaction filtering
#[derive(Debug, Deserialize)]
pub struct TransactionQueryParams {
    pub transaction_type: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_attempts: Option<i32>,
    pub has_signature: Option<bool>,
}

/// Get transactions for authenticated user
#[utoipa::path(
    get,
    path = "/api/v1/transactions/user",
    tag = "transactions",
    summary = "Get user transactions",
    description = "Retrieve a paginated list of transactions for authenticated user with optional filters",
    params(
        ("transaction_type" = Option<String>, Query, description = "Filter by transaction type"),
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

    // Convert query parameters to filters
    let filters = params.into_transaction_filters(Some(user.sub));

    let transactions = app_state
        .transaction_coordinator
        .get_user_transactions(user.sub, filters)
        .await?;

    debug!("Retrieved {} transactions for user", transactions.len());
    Ok(Json(transactions))
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
        ("transaction_type" = Option<String>, Query, description = "Filter by transaction type"),
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

    let filters = params.into_transaction_filters(None);
    let transactions = app_state
        .transaction_coordinator
        .get_transactions(filters)
        .await?;

    debug!("Retrieved {} transactions", transactions.len());
    Ok(Json(transactions))
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

    let stats = app_state
        .transaction_coordinator
        .get_transaction_stats()
        .await?;

    debug!("Retrieved transaction stats: {:?}", stats);
    Ok(Json(stats))
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

    // Get transaction details to check permissions
    let transaction = app_state
        .transaction_coordinator
        .get_transaction_status(id)
        .await?;

    // Check if user has permission to retry this transaction
    match transaction.user_id {
        Some(transaction_user_id) => {
            // Users can only retry their own transactions
            if transaction_user_id != user.sub {
                return Err(ApiError::Forbidden(
                    "You can only retry your own transactions".to_string(),
                ));
            }
        }
        None => {
            // If no user_id, only admin can retry
            if user.role != "admin" {
                return Err(ApiError::Forbidden(
                    "Admin access required to retry this transaction".to_string(),
                ));
            }
        }
    }

    // Merge ID from path with the request
    let retry_request = TransactionRetryRequest {
        operation_id: id,
        operation_type: request.operation_type,
        max_attempts: request.max_attempts,
    };

    let result = app_state
        .transaction_coordinator
        .retry_transaction(retry_request)
        .await?;

    info!(
        "Transaction retry completed with success: {}, attempts: {}",
        result.success, result.attempts
    );

    Ok(Json(result))
}

impl TransactionQueryParams {
    /// Convert query parameters to TransactionFilters
    pub fn into_transaction_filters(self, user_id: Option<Uuid>) -> crate::models::transaction::TransactionFilters {
        use chrono::{DateTime, Utc};

        crate::models::transaction::TransactionFilters {
            transaction_type: self.transaction_type,
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
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/mod.rs
</file_path>

<edit_description>
Update transactions mod to fix compilation errors
</edit_description>
```rust
// Transaction Handlers
// API endpoints for unified blockchain transaction tracking

pub mod create;
pub mod status;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use tracing::{debug, info};

use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    TransactionFilters, TransactionResponse, TransactionRetryRequest, TransactionRetryResponse,
    TransactionStats, TransactionType,
};

// Re-export handlers
pub use create::{
    create_transaction, get_user_transactions, get_transaction_history,
    get_transaction_stats, retry_transaction, TransactionQueryParams,
};
pub use status::get_transaction_status;
```

<file_path>
gridtokenx-apigateway/src/main.rs
</file_path>

<edit_description>
Update main.rs to register transaction handlers
</edit_description>
```rust
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

mod handlers;
mod auth;
mod error;
mod middleware;
mod models;
mod services;
mod utils;

// Database
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

// Web framework
use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
    Json,
};
use axum::middleware;

// Application state
use auth::middleware::{AuthenticatedUser, JwtAuthorizer};
use handlers::{
    admin, analytics, audit, auth, blockchain, energy_trading, epochs, erc,
    governance, health, market_data, meter_verification, meters, metrics, oracle,
    registry, token, trading, transactions, user_management, wallet_auth, websocket,
};
use models::user::Claims;
use services::{
    cache_service::CacheService, settlement_service::SettlementService,
    token_service::TokenService, transaction_coordinator::TransactionCoordinator,
    transaction_validation_service::TransactionValidationService,
    wallet_service::WalletService,
};

// OpenAPI
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Set up tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("Starting GridTokenX API Gateway");

    // Initialize database
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let db = PgPool::connect_with(&PgPoolOptions::new().max_connections(20), &database_url)
        .await
        .expect("Failed to connect to Postgres");

    info!("Connected to database");

    // Initialize services
    let cache_service = CacheService::new()?;
    let wallet_service = WalletService::new()?;
    let token_service = TokenService::new(db.clone())?;
    let settlement_service = SettlementService::new(db.clone(), token_service.clone())?;

    // Initialize blockchain service
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.devnet.solana.com".to_string());
    let cluster = std::env::var("SOLANA_CLUSTER")
        .unwrap_or_else(|_| "devnet".to_string());

    let blockchain_service = services::BlockchainService::new(rpc_url, cluster)?;

    // Initialize validation services
    let validation_service = TransactionValidationService::new(
        // For simplicity, we'll use the existing services
        // In a real implementation, you would inject actual service implementations
    );

    // Initialize transaction coordinator
    let transaction_coordinator = TransactionCoordinator::new(
        db.clone(),
        std::sync::Arc::new(blockchain_service),
        std::sync::Arc::new(settlement_service),
        std::sync::Arc::new(validation_service),
    );

    // Initialize JWT authorizer
    let jwt_secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set");
    let jwt_authorizer = JwtAuthorizer::new(&jwt_secret);

    // Create application state
    let app_state = AppState {
        db: db.clone(),
        cache_service: cache_service.clone(),
        wallet_service: wallet_service.clone(),
        token_service: token_service.clone(),
        settlement_service: settlement_service.clone(),
        blockchain_service: std::sync::Arc::new(blockchain_service),
        transaction_coordinator: std::sync::Arc::new(transaction_coordinator),
        validation_service: std::sync::Arc::new(validation_service),
        jwt_authorizer,
    };

    // Create API routes
    let api_routes = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/api/v1/auth/login", post(handlers::auth::login))
        .route(
            "/api/v1/auth/refresh",
            post(handlers::auth::refresh),
        )
        .route(
            "/api/v1/wallet/auth",
            post(handlers::wallet_auth::authenticate_wallet),
        )
        .route(
            "/api/v1/users",
            post(handlers::user_management::create_user),
        )
        .route(
            "/api/v1/users/me",
            get(handlers::user_management::get_user),
        )
        .route(
            "/api/v1/meters",
            post(handlers::meters::register_meter),
        )
        .route(
            "/api/v1/meters/:id",
            get(handlers::meters::get_meter),
        )
        .route(
            "/api/v1/meters/:id/verify",
            post(handlers::meter_verification::verify_meter),
        )
        .route(
            "/api/v1/meters/:id/reading",
            post(handlers::meters::add_meter_reading),
        )
        .route(
            "/api/v1/transactions",
            post(transactions::create_transaction),
        )
        .route(
            "/api/v1/transactions/:id/status",
            get(transactions::get_transaction_status),
        )
        .route(
            "/api/v1/transactions/user",
            get(transactions::get_user_transactions),
        )
        .route(
            "/api/v1/transactions/history",
            get(transactions::get_transaction_history),
        )
        .route(
            "/api/v1/transactions/stats",
            get(transactions::get_transaction_stats),
        )
        .route(
            "/api/v1/transactions/:id/retry",
            post(transactions::retry_transaction),
        )
        .route(
            "/api/v1/erc",
            post(handlers::erc::create_erc),
        )
        .route(
            "/api/v1/erc/:id/validate",
            post(handlers::erc::validate_erc),
        )
        .route(
            "/api/v1/erc/:id",
            get(handlers::erc::get_erc),
        )
        .route(
            "/api/v1/energy-tokens/balance/:user_id",
            get(handlers::token::get_user_token_balance),
        )
        .route(
            "/api/v1/energy-tokens/transfer",
            post(handlers::token::transfer_tokens),
        )
        .route(
            "/api/v1/energy-tokens/mint",
            post(handlers::token::mint_tokens),
        )
        .route(
            "/api/v1/trading/orders",
            post(handlers::trading::create_order),
        )
        .route(
            "/api/v1/trading/orders/:id",
            get(handlers::trading::get_order),
        )
        .route(
            "/api/v1/trading/orders/:id/cancel",
            post(handlers::trading::cancel_order),
        )
        .route(
            "/api/v1/trading/market",
            get(handlers::energy_trading::get_market),
        )
        .route(
            "/api/v1/trading/market/orders",
            get(handlers::energy_trading::get_market_orders),
        )
        .route(
            "/api/v1/trading/orders/match",
            post(handlers::energy_trading::match_orders),
        )
        .route(
            "/api/v1/governance/proposals",
            get(handlers::governance::get_proposals),
        )
        .route(
            "/api/v1/governance/proposals/:id",
            get(handlers::governance::get_proposal),
        )
        .route(
            "/api/v1/governance/proposals/:id/vote",
            post(handlers::governance::vote),
        )
        .route(
            "/api/v1/oracle/price-feeds",
            get(handlers::oracle::get_price_feeds),
        )
        .route(
            "/api/v1/oracle/price-feeds/:id",
            get(handlers::oracle::get_price_feed),
        )
        .route(
            "/api/v1/oracle/price-feeds/:id/update",
            post(handlers::oracle::update_price_feed),
        )
        .route(
            "/api/v1/registry/participants",
            get(handlers::registry::get_participants),
        )
        .route(
            "/api/v1/registry/participants/:id",
            get(handlers::registry::get_participant),
        )
        .route(
            "/api/v1/registry/participants/:id",
            post(handlers::registry::update_participant),
        )
        .route(
            "/api/v1/settlements",
            get(handlers::settlement::get_settlements),
        )
        .route(
            "/api/v1/settlements/:id",
            get(handlers::settlement::get_settlement),
        )
        .route(
            "/api/v1/epochs/current",
            get(handlers::epochs::get_current_epoch),
        )
        .route(
            "/api/v1/epochs/history",
            get(handlers::epochs::get_epoch_history),
        )
        .route(
            "/api/v1/audit/log",
            post(handlers::audit::log_action),
        )
        .route(
            "/api/v1/audit/history",
            get(handlers::audit::get_audit_history),
        )
        .route(
            "/api/v1/metrics/transactions",
            get(handlers::metrics::get_transaction_metrics),
        )
        .route(
            "/api/v1/admin/analytics",
            get(handlers::analytics::get_analytics),
        );

    // Create OpenAPI documentation
    let openapi = OpenApi::new(
        "GridTokenX API Gateway",
        "1.0.0",
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
    )
    .merge(
        JsonSchema {
            title: Some("GridTokenX API Gateway".to_string()),
            description: Some(
                "API Gateway for GridTokenX Energy Trading System".to_string(),
            ),
            version: Some("1.0.0".to_string()),
            ..Default::default()
        }
        .into()
    )
    .paths(
        handlers::auth::login,
        handlers::auth::refresh,
        handlers::user_management::create_user,
        handlers::user_management::get_user,
        handlers::meters::register_meter,
        handlers::meters::get_meter,
        handlers::meter_verification::verify_meter,
        handlers::meters::add_meter_reading,
        handlers::token::get_user_token_balance,
        handlers::token::transfer_tokens,
        handlers::token::mint_tokens,
        handlers::transactions::create_transaction,
        handlers::transactions::get_transaction_status,
        handlers::transactions::get_user_transactions,
        handlers::transactions::get_transaction_history,
        handlers::transactions::get_transaction_stats,
        handlers::transactions::retry_transaction,
        handlers::erc::create_erc,
        handlers::erc::validate_erc,
        handlers::erc::get_erc,
        handlers::trading::create_order,
        handlers::trading::get_order,
        handlers::trading::cancel_order,
        handlers::energy_trading::get_market,
        handlers::energy_trading::get_market_orders,
        handlers::energy_trading::match_orders,
        handlers::governance::get_proposals,
        handlers::governance::get_proposal,
        handlers::governance::vote,
        handlers::oracle::get_price_feeds,
        handlers::oracle::get_price_feed,
        handlers::oracle::update_price_feed,
        handlers::registry::get_participants,
        handlers::registry::get_participant,
        handlers::registry::update_participant,
        handlers::settlement::get_settlements,
        handlers::settlement::get_settlement,
        handlers::epochs::get_current_epoch,
        handlers::epochs::get_epoch_history,
        handlers::audit::log_action,
        handlers::audit::get_audit_history,
        handlers::metrics::get_transaction_metrics,
        handlers::analytics::get_analytics,
    )
    .tags(
        handlers::auth::login,
        handlers::auth::refresh,
        handlers::user_management::create_user,
        handlers::user_management::get_user,
        handlers::meters::register_meter,
        handlers::meters::get_meter,
        handlers::meter_verification::VerifyMeterRequest,
        handlers::meter_verification::verify_meter,
        handlers::meters::AddMeterReadingRequest,
        handlers::meters::add_meter_reading,
        handlers::token::GetUserTokenBalanceResponse,
        handlers::token::get_user_token_balance,
        handlers::token::TransferTokensRequest,
        handlers::token::transfer_tokens,
        handlers::token::MintTokensRequest,
        handlers::token::mint_tokens,
        handlers::transactions::CreateTransactionRequest,
        handlers::transactions::create_transaction,
        handlers::TransactionResponse,
        handlers::transactions::get_transaction_status,
        handlers::TransactionQueryParams,
        handlers::transactions::get_user_transactions,
        handlers::transactions::get_transaction_history,
        handlers::transactions::get_transaction_stats,
        handlers::TransactionRetryRequest,
        handlers::transactions::retry_transaction,
        handlers::erc::CreateErcRequest,
        handlers::erc::create_erc,
        handlers::erc::ValidateErcRequest,
        handlers::erc::validate_erc,
        handlers::erc::ErcCertificateResponse,
        handlers::erc::get_erc,
        handlers::trading::CreateOrderRequest,
        handlers::trading::create_order,
        handlers::trading::OrderResponse,
        handlers::trading::get_order,
        handlers::trading::CancelOrderRequest,
        handlers::trading::cancel_order,
        handlers::energy_trading::MarketResponse,
        handlers::energy_trading::get_market,
        handlers::energy_trading::OrderResponse,
        handlers::energy_trading::get_market_orders,
        handlers::trading::MatchOrdersRequest,
        handlers::energy_trading::match_orders,
        handlers::governance::Proposal,
        handlers::governance::get_proposals,
        handlers::governance::ProposalResponse,
        handlers::governance::get_proposal,
        handlers::governance::VoteRequest,
        handlers::governance::vote,
        handlers::oracle::PriceFeedResponse,
        handlers::oracle::get_price_feeds,
        handlers::oracle::PriceFeedResponse,
        handlers::oracle::get_price_feed,
        handlers::oracle::UpdatePriceFeedRequest,
        handlers::oracle::update_price_feed,
        handlers::registry::Participant,
        handlers::registry::get_participants,
        handlers::registry::ParticipantResponse,
        handlers::registry::get_participant,
        handlers::registry::UpdateParticipantRequest,
        handlers::registry::update_participant,
        handlers::settlement::Settlement,
        handlers::settlement::get_settlements,
        handlers::settlement::SettlementResponse,
        handlers::settlement::get_settlement,
        handlers::epochs::Epoch,
        handlers::epochs::get_current_epoch,
        handlers::epochs::EpochResponse,
        handlers::epochs::get_epoch_history,
        handlers::audit::AuditEventRecord,
        handlers::audit::log_action,
        handlers::audit::AuditHistoryResponse,
        handlers::audit::get_audit_history,
        handlers::metrics::TransactionMetrics,
        handlers::metrics::get_transaction_metrics,
        handlers::analytics::AnalyticsResponse,
        handlers::analytics::get_analytics,
    );

    // Create Swagger UI
    let swagger_ui = SwaggerUi::new("/api-docs/openapi.json", openapi);

    // Create app with middleware
    let app = Router::new()
        .merge(api_routes)
        .merge(swagger_ui)
        .layer(
            middleware::from_fn(middleware::cors::cors)
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &axum::http::Request<_>| {
                            tracing::info_span!("http_request",
                                method = %request.method(),
                                uri = %request.uri(),
                                version = %request.version()
                            )
                        }),
                ),
        )
        .layer(middleware::from_fn(middleware::auth::auth))
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::request_id::request_id,
        ))
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::request_logging::request_logging,
        ))
        .with_state(app_state);

    // Set up graceful shutdown
    let graceful = axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024); // 2MB

    // Bind listener
    let listener = TcpListener::bind(&SocketAddr::from((
        [0, 0, 0, 0],
        8080,
    )))
    .await?;

    info!("API Gateway listening on http://0.0.0.0:8080");

    // Start server
    axum::serve(listener, graceful.into_make_service())
        .await?;

    Ok(())
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cache_service: CacheService,
    pub wallet_service: WalletService,
    pub token_service: TokenService,
    pub settlement_service: SettlementService,
    pub blockchain_service: std::sync::Arc<services::BlockchainService>,
    pub transaction_coordinator: std::sync::Arc<TransactionCoordinator>,
    pub validation_service: std::sync::Arc<TransactionValidationService>,
    pub jwt_authorizer: auth::middleware::JwtAuthorizer,
}
```

<file_path>
gridtokenx-apigateway/migrations/20231201120000_create_blockchain_operations.sql
</file_path>

<edit_description>
Create database migration for Phase 1
</edit_description>
```sql
-- Create blockchain_operations table for transaction tracking
CREATE TABLE IF NOT EXISTS blockchain_operations (
    operation_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operation_type VARCHAR(50) NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    signature VARCHAR(100),
    tx_type VARCHAR(50),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    payload JSONB NOT NULL,
    max_priority_fee BIGINT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    submitted_at TIMESTAMP WITH TIME ZONE,
    confirmed_at TIMESTAMP WITH TIME ZONE
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_status ON blockchain_operations(status);
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_user_id ON blockchain_operations(user_id);
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_operation_type ON blockchain_operations(operation_type);
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_created_at ON blockchain_operations(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_signature ON blockchain_operations(signature) WHERE signature IS NOT NULL;

-- Create trigger to automatically update updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_blockchain_operations_updated_at
    BEFORE UPDATE ON blockchain_operations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Add a comment describing the table
COMMENT ON TABLE blockchain_operations IS 'Stores all blockchain transactions with their lifecycle status';
```

<file_path>
gridtokenx-apigateway/migrations/20231201120001_add_settled_status.sql
</file_path>

<edit_description>
Add settled status to transaction operations
</edit_description>
```sql
-- Add settled status to transaction operations table
ALTER TABLE blockchain_operations
ADD COLUMN IF NOT EXISTS settled_at TIMESTAMP WITH TIME ZONE;

-- Create index for settled_at
CREATE INDEX IF NOT EXISTS idx_blockchain_operations_settled_at ON blockchain_operations(settled_at DESC) WHERE settled_at IS NOT NULL;

-- Add a comment for the new column
COMMENT ON COLUMN blockchain_operations.settled_at IS 'Timestamp when the transaction was fully settled';
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/mod.rs
</file_path>

<edit_description>
Fix imports in transactions mod
</edit_description>
```rust
// Transaction Handlers
// API endpoints for unified blockchain transaction tracking

pub mod create;
pub mod status;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use tracing::{debug, info};

use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    TransactionFilters, TransactionResponse, TransactionRetryRequest, TransactionRetryResponse,
    TransactionStats,
};

// Re-export handlers
pub use create::{
    create_transaction, get_user_transactions, get_transaction_history,
    get_transaction_stats, retry_transaction, TransactionQueryParams,
};
pub use status::get_transaction_status;
```

<file_path>
gridtokenx-apigateway/src/handlers/transactions/create.rs
</file_path>

<edit_description>
Fix import issue in create handler
</edit_description>
```rust
// Transaction Creation Handler
// API endpoint for creating and submitting blockchain transactions

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::transaction::{
    CreateTransactionRequest, TransactionResponse, TransactionType,
};
use crate::services::transaction_coordinator::TransactionCoordinator;
use crate::services::validation::TransactionValidationService;
