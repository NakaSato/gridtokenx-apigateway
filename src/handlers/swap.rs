use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::amm::SwapQuote;
use crate::services::amm_service::SwapTransaction;
use axum::{
    Json,
    extract::{Query, State},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError};

fn validate_positive_decimal(amount: &Decimal) -> Result<(), ValidationError> {
    if amount <= &Decimal::ZERO {
        return Err(ValidationError::new("amount_must_be_positive"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
pub struct QuoteRequest {
    pub pool_id: Uuid,
    #[validate(length(min = 1))]
    pub input_token: String,
    #[validate(custom(function = "validate_positive_decimal"))]
    pub input_amount: Decimal,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ExecuteSwapRequest {
    pub pool_id: Uuid,
    #[validate(length(min = 1))]
    pub input_token: String,
    #[validate(custom(function = "validate_positive_decimal"))]
    pub input_amount: Decimal,
    #[validate(custom(function = "validate_positive_decimal"))]
    pub min_output_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct PoolResponse {
    pub id: Uuid,
    pub token_a: String,
    pub token_b: String,
    pub reserve_a: Decimal,
    pub reserve_b: Decimal,
    pub fee_rate: Decimal,
}

/// Get a quote for a swap
pub async fn get_quote(
    State(state): State<AppState>,
    Json(payload): Json<QuoteRequest>,
) -> Result<Json<SwapQuote>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let quote = state
        .amm_service
        .calculate_swap_output(payload.pool_id, &payload.input_token, payload.input_amount)
        .await?;

    Ok(Json(quote))
}

/// Execute a swap
pub async fn execute_swap(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<ExecuteSwapRequest>,
) -> Result<Json<SwapTransaction>, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let transaction = state
        .amm_service
        .execute_swap(
            user.sub,
            payload.pool_id,
            payload.input_token,
            payload.input_amount,
            payload.min_output_amount,
        )
        .await?;

    Ok(Json(transaction))
}

/// List all available liquidity pools
pub async fn list_pools(
    State(state): State<AppState>,
) -> Result<Json<Vec<PoolResponse>>, ApiError> {
    let pools = state.amm_service.list_pools().await?;

    let response = pools
        .into_iter()
        .map(|p| PoolResponse {
            id: p.id,
            token_a: p.token_a,
            token_b: p.token_b,
            reserve_a: p.reserve_a,
            reserve_b: p.reserve_b,
            fee_rate: p.fee_rate,
        })
        .collect();

    Ok(Json(response))
}

/// Get user's swap history
pub async fn get_swap_history(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Json<Vec<SwapTransaction>>, ApiError> {
    let history = state.amm_service.get_user_swap_history(user.sub).await?;
    Ok(Json(history))
}
