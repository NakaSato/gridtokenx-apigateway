// Blockchain Testing Handlers
// These endpoints are for testing blockchain functionality in development/staging environments

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::middleware::AuthenticatedUser,
    error::ApiError,
    AppState,
};

/// Test transaction request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTestTransactionRequest {
    /// Test transaction type
    pub transaction_type: String,
    /// Optional test data
    pub test_data: Option<serde_json::Value>,
}

/// Test transaction response
#[derive(Debug, Serialize, ToSchema)]
pub struct TestTransactionResponse {
    pub id: String,
    pub signature: String,
    pub status: String,
    pub transaction_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Test statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct TestStatisticsResponse {
    pub total_test_transactions: i64,
    pub successful_transactions: i64,
    pub failed_transactions: i64,
    pub average_execution_time_ms: f64,
}

/// Create a test transaction
/// POST /api/test/transactions
/// 
/// For testing blockchain functionality in non-production environments
#[utoipa::path(
    post,
    path = "/api/test/transactions",
    tag = "testing",
    request_body = CreateTestTransactionRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Test transaction created", body = TestTransactionResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_test_transaction(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<CreateTestTransactionRequest>,
) -> Result<Json<TestTransactionResponse>, ApiError> {
    info!(
        "Creating test transaction for user {}: type={}",
        user.sub, request.transaction_type
    );

    // Generate test transaction ID
    let transaction_id = Uuid::new_v4().to_string();
    let signature = format!("TEST_{}", Uuid::new_v4());

    // In a real implementation, this would interact with the blockchain
    // For now, we'll just return a mock response
    let response = TestTransactionResponse {
        id: transaction_id,
        signature: signature.clone(),
        status: "pending".to_string(),
        transaction_type: request.transaction_type,
        created_at: chrono::Utc::now(),
    };

    info!(
        "Test transaction created: {} (signature: {})",
        response.id, signature
    );

    Ok(Json(response))
}

/// Get test transaction status
/// GET /api/test/transactions/{signature}
#[utoipa::path(
    get,
    path = "/api/test/transactions/{signature}",
    tag = "testing",
    params(
        ("signature" = String, Path, description = "Transaction signature")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Transaction status retrieved", body = TestTransactionResponse),
        (status = 404, description = "Transaction not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_test_transaction_status(
    State(_state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
    Path(signature): Path<String>,
) -> Result<Json<TestTransactionResponse>, ApiError> {
    info!("Fetching test transaction status: {}", signature);

    // Mock response - in production this would query the blockchain
    let response = TestTransactionResponse {
        id: Uuid::new_v4().to_string(),
        signature: signature.clone(),
        status: "confirmed".to_string(),
        transaction_type: "test".to_string(),
        created_at: chrono::Utc::now(),
    };

    Ok(Json(response))
}

/// Get test statistics
/// GET /api/test/statistics
#[utoipa::path(
    get,
    path = "/api/test/statistics",
    tag = "testing",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Test statistics retrieved", body = TestStatisticsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_test_statistics(
    State(_state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
) -> Result<Json<TestStatisticsResponse>, ApiError> {
    info!("Fetching test statistics");

    // Mock statistics
    let response = TestStatisticsResponse {
        total_test_transactions: 100,
        successful_transactions: 95,
        failed_transactions: 5,
        average_execution_time_ms: 250.5,
    };

    Ok(Json(response))
}
