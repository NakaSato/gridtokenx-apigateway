//! Carbon Credits Handler
//!
//! Track and trade carbon credits from renewable energy

use axum::{extract::{State, Query}, response::Json};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use tracing::{info, error};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Carbon credit status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "carbon_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CarbonStatus {
    Active,
    Retired,
    Transferred,
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "carbon_transaction_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CarbonTransactionStatus {
    Pending,
    Completed,
    Failed,
    Cancelled,
}

/// Carbon credit record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CarbonCredit {
    pub id: Uuid,
    pub user_id: Uuid,
    #[schema(value_type = String)]
    pub amount: Decimal,
    pub source: String,
    pub source_reference_id: Option<Uuid>,
    pub status: CarbonStatus,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Carbon transaction record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CarbonTransaction {
    pub id: Uuid,
    pub from_user_id: Option<Uuid>,
    pub to_user_id: Uuid,
    #[schema(value_type = String)]
    pub amount: Decimal,
    #[schema(value_type = String)]
    pub price_per_credit: Option<Decimal>,
    #[schema(value_type = String)]
    pub total_value: Option<Decimal>,
    pub status: CarbonTransactionStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Carbon balance response
#[derive(Debug, Serialize, ToSchema)]
pub struct CarbonBalanceResponse {
    #[schema(value_type = String)]
    pub total_credits: Decimal,
    #[schema(value_type = String)]
    pub active_credits: Decimal,
    #[schema(value_type = String)]
    pub retired_credits: Decimal,
    #[schema(value_type = String)]
    pub transferred_credits: Decimal,
    /// Equivalent in kg CO2
    #[schema(value_type = f64)]
    pub kg_co2_equivalent: f64,
}

/// Credit history query params
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Transfer request
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct TransferRequest {
    /// Recipient's wallet address or user ID
    pub to_wallet_address: String,
    
    /// Amount of credits to transfer
    #[schema(value_type = String)]
    pub amount: Decimal,
    
    /// Optional notes
    pub notes: Option<String>,
}

/// Get carbon credit balance
/// GET /api/v1/carbon/balance
#[utoipa::path(
    get,
    path = "/api/v1/carbon/balance",
    tag = "carbon",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Carbon credit balance", body = CarbonBalanceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_carbon_balance(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<CarbonBalanceResponse>> {
    let balance = sqlx::query!(
        r#"
        WITH combined AS (
            SELECT 
                COALESCE(SUM(amount), 0) as total,
                COALESCE(SUM(CASE WHEN status = 'active' THEN amount ELSE 0 END), 0) as active,
                COALESCE(SUM(CASE WHEN status = 'retired' THEN amount ELSE 0 END), 0) as retired,
                COALESCE(SUM(CASE WHEN status = 'transferred' THEN amount ELSE 0 END), 0) as transferred
            FROM carbon_credits
            WHERE user_id = $1
            UNION ALL
            SELECT 
                COALESCE(SUM(kwh_amount), 0) as total,
                COALESCE(SUM(CASE WHEN status = 'active' THEN kwh_amount ELSE 0 END), 0) as active,
                COALESCE(SUM(CASE WHEN status = 'retired' THEN kwh_amount ELSE 0 END), 0) as retired,
                0 as transferred
            FROM erc_certificates
            WHERE user_id = $1
        )
        SELECT 
            SUM(total) as "total!",
            SUM(active) as "active!",
            SUM(retired) as "retired!",
            SUM(transferred) as "transferred!"
        FROM combined
        "#,
        user.0.sub
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to get carbon balance: {}", e);
        ApiError::Internal(format!("Failed to get balance: {}", e))
    })?;

    // Convert tons to kg (1 ton = 1000 kg)
    let active_f64 = balance.active.to_string().parse::<f64>().unwrap_or(0.0);
    
    Ok(Json(CarbonBalanceResponse {
        total_credits: balance.total,
        active_credits: balance.active,
        retired_credits: balance.retired,
        transferred_credits: balance.transferred,
        kg_co2_equivalent: active_f64 * 1000.0,
    }))
}

/// Get carbon credit history
/// GET /api/v1/carbon/history
#[utoipa::path(
    get,
    path = "/api/v1/carbon/history",
    tag = "carbon",
    params(
        ("limit" = Option<i64>, Query, description = "Max records to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Credit history", body = Vec<CarbonCredit>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_carbon_history(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Vec<CarbonCredit>>> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let credits = sqlx::query_as::<_, CarbonCredit>(
        r#"
        SELECT id, user_id, amount, source, source_reference_id, status as status, description, created_at
        FROM (
            SELECT id, user_id, amount, source, source_reference_id,
                   status::text as status,
                   description, created_at
            FROM carbon_credits
            WHERE user_id = $1
            UNION ALL
            SELECT id, user_id, kwh_amount as amount, 
                   'REC' as source, settlement_id as source_reference_id,
                   status::text as status,
                   certificate_id as description, created_at
            FROM erc_certificates
            WHERE user_id = $1
        ) combined
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#
    )
    .bind(user.0.sub)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to get carbon history: {}", e);
        ApiError::Internal(format!("Failed to get history: {}", e))
    })?;

    Ok(Json(credits))
}

/// Transfer carbon credits to another user
/// POST /api/v1/carbon/transfer
#[utoipa::path(
    post,
    path = "/api/v1/carbon/transfer",
    tag = "carbon",
    request_body = TransferRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Transfer completed", body = CarbonTransaction),
        (status = 400, description = "Insufficient credits or invalid recipient"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn transfer_credits(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<TransferRequest>,
) -> Result<Json<CarbonTransaction>> {
    info!("Transferring {} carbon credits from user {}", payload.amount, user.0.sub);

    if payload.amount <= Decimal::ZERO {
        return Err(ApiError::BadRequest("Amount must be positive".to_string()));
    }

    // Find recipient by wallet address
    let recipient = sqlx::query!(
        "SELECT id FROM users WHERE wallet_address = $1",
        payload.to_wallet_address
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let recipient_id = match recipient {
        Some(r) => r.id,
        None => return Err(ApiError::BadRequest("Recipient not found".to_string())),
    };

    if recipient_id == user.0.sub {
        return Err(ApiError::BadRequest("Cannot transfer to yourself".to_string()));
    }

    // Begin transaction
    let mut tx = state.db.begin().await.map_err(|e| ApiError::Internal(e.to_string()))?;

    // Check sender's active balance
    let sender_balance: Decimal = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(amount), 0) as "balance!" FROM carbon_credits WHERE user_id = $1 AND status = 'active'"#,
        user.0.sub
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    if sender_balance < payload.amount {
        return Err(ApiError::BadRequest(format!(
            "Insufficient credits. Available: {}, Requested: {}",
            sender_balance, payload.amount
        )));
    }

    // Mark credits as transferred (deduct from sender)
    // In a real system, you'd track which specific credits are transferred
    sqlx::query!(
        r#"
        INSERT INTO carbon_credits (user_id, amount, source, status, description)
        VALUES ($1, $2, 'transfer_out', 'transferred', $3)
        "#,
        user.0.sub,
        -payload.amount,  // Negative amount for deduction
        format!("Transfer to {}", payload.to_wallet_address)
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Add credits to recipient
    sqlx::query!(
        r#"
        INSERT INTO carbon_credits (user_id, amount, source, status, description)
        VALUES ($1, $2, 'transfer_in', 'active', $3)
        "#,
        recipient_id,
        payload.amount,
        format!("Transfer from user")
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Record the transaction
    let transaction = sqlx::query_as!(
        CarbonTransaction,
        r#"
        INSERT INTO carbon_transactions (from_user_id, to_user_id, amount, status, notes)
        VALUES ($1, $2, $3, 'completed', $4)
        RETURNING id, from_user_id, to_user_id, amount, price_per_credit, total_value,
                  status as "status!: CarbonTransactionStatus", notes, created_at as "created_at!"
        "#,
        user.0.sub,
        recipient_id,
        payload.amount,
        payload.notes
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    tx.commit().await.map_err(|e| ApiError::Internal(e.to_string()))?;

    info!("Transferred {} credits from {} to {}", payload.amount, user.0.sub, recipient_id);

    Ok(Json(transaction))
}

/// Get carbon transaction history
/// GET /api/v1/carbon/transactions
pub async fn get_carbon_transactions(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Vec<CarbonTransaction>>> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let transactions = sqlx::query_as!(
        CarbonTransaction,
        r#"
        SELECT id, from_user_id, to_user_id, amount, price_per_credit, total_value,
               status as "status!: CarbonTransactionStatus", notes, created_at as "created_at!"
        FROM carbon_transactions
        WHERE from_user_id = $1 OR to_user_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        user.0.sub,
        limit,
        offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to get carbon transactions: {}", e);
        ApiError::Internal(format!("Failed to get transactions: {}", e))
    })?;

    Ok(Json(transactions))
}
