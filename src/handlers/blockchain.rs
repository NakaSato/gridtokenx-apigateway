use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::blockchain::{ProgramInteraction, TransactionStatus, TransactionSubmission};

/// Query parameters for transaction history
#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
pub struct TransactionQuery {
    pub program_id: Option<String>,
    pub status: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// Response for transaction submission
#[derive(Debug, Serialize, ToSchema)]
pub struct TransactionResponse {
    pub signature: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub estimated_confirmation_time: i32, // seconds
}

/// Account information response
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountInfo {
    pub address: String,
    #[schema(value_type = String)]
    pub balance: rust_decimal::Decimal,
    pub executable: bool,
    pub owner: String,
    pub rent_epoch: u64,
    pub data_length: usize,
}

/// Network status response
#[derive(Debug, Serialize, ToSchema)]
pub struct NetworkStatus {
    pub cluster: String,
    pub block_height: u64,
    pub block_time: DateTime<Utc>,
    pub tps: f64,
    pub health: String,
    pub version: String,
}

/// Program interaction request
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ProgramInteractionRequest {
    pub program_name: String,
    pub instruction: String,
    pub accounts: Vec<String>,
    pub data: serde_json::Value,
    #[validate(range(min = 1000, max = 1000000))]
    pub compute_units: Option<u32>,
}

/// Submit a blockchain transaction
/// POST /api/blockchain/transactions
#[utoipa::path(
    post,
    path = "/api/blockchain/transactions",
    tag = "blockchain",
    request_body = TransactionSubmission,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Transaction submitted successfully", body = TransactionResponse),
        (status = 400, description = "Invalid transaction data"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_transaction(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<TransactionSubmission>,
) -> Result<Json<TransactionResponse>> {
    tracing::info!("Submitting blockchain transaction for user: {}", user.0.sub);

    // Decode transaction
    let tx_bytes = general_purpose::STANDARD
        .decode(&payload.transaction)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64 transaction: {}", e)))?;

    let transaction: Transaction = bincode::deserialize(&tx_bytes)
        .map_err(|e| ApiError::BadRequest(format!("Invalid transaction data: {}", e)))?;

    // Submit to blockchain
    let signature = state
        .blockchain_service
        .submit_transaction(transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to submit transaction: {}", e);
            ApiError::Internal(format!("Blockchain submission failed: {}", e))
        })?;

    let signature_str = signature.to_string();

    // Store transaction record in database
    let fee_lamports = payload.priority_fee.to_string().parse::<i64>().unwrap_or(0);

    sqlx::query!(
        r#"
        INSERT INTO blockchain_transactions 
        (signature, user_id, program_id, instruction_name, status, fee, compute_units_consumed, submitted_at)
        VALUES ($1, $2, $3, $4, 'Pending', $5, $6, NOW())
        "#,
        signature_str,
        user.0.sub,
        payload.program_id,
        "submit_transaction".to_string(),
        fee_lamports,
        payload.compute_units as i32
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to store transaction record: {}", e);
        ApiError::Database(e)
    })?;

    let response = TransactionResponse {
        signature: signature_str.clone(),
        status: "pending".to_string(),
        submitted_at: Utc::now(),
        estimated_confirmation_time: 5, // Solana is fast
    };

    tracing::info!("Transaction submitted successfully: {}", signature_str);
    Ok(Json(response))
}

/// Get transaction history for authenticated user
/// GET /api/blockchain/transactions
#[utoipa::path(
    get,
    path = "/api/blockchain/transactions",
    tag = "blockchain",
    params(TransactionQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's blockchain transactions", body = Vec<TransactionStatus>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_transaction_history(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<TransactionQuery>,
) -> Result<Json<Vec<TransactionStatus>>> {
    tracing::info!("Fetching transaction history for user: {}", user.0.sub);

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let mut query = "SELECT * FROM blockchain_transactions WHERE user_id = $1".to_string();
    let mut param_count = 1;
    let mut query_params: Vec<String> = vec![user.0.sub.to_string()];

    // Add optional filters
    if let Some(program_id) = &params.program_id {
        param_count += 1;
        query.push_str(&format!(" AND program_id = ${}", param_count));
        query_params.push(program_id.clone());
    }

    if let Some(status) = &params.status {
        param_count += 1;
        query.push_str(&format!(" AND status = ${}", param_count));
        query_params.push(status.clone());
    }

    query.push_str(" ORDER BY created_at DESC");
    query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    // Simulate transaction status retrieval
    // In production, this would query actual blockchain data
    let transactions = vec![TransactionStatus {
        signature: "tx_sample_12345".to_string(),
        status: "confirmed".to_string(),
        block_height: Some(1000000),
        confirmation_status: "finalized".to_string(),
        fee: rust_decimal::Decimal::new(5000, 9), // 0.000005 SOL
        compute_units_consumed: Some(5000),
        logs: vec!["Program log: Instruction processed successfully".to_string()],
        program_interactions: vec![ProgramInteraction {
            program_id: "EnergyTradingProgram".to_string(),
            instruction_name: "place_order".to_string(),
            success: true,
        }],
    }];

    Ok(Json(transactions))
}

/// Get specific transaction status by signature
/// GET /api/blockchain/transactions/:signature
#[utoipa::path(
    get,
    path = "/api/blockchain/transactions/{signature}",
    tag = "blockchain",
    security(("bearer_auth" = [])),
    params(
        ("signature" = String, Path, description = "Transaction signature")
    ),
    responses(
        (status = 200, description = "Transaction status details", body = TransactionStatus),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Transaction not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_transaction_status(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(signature): Path<String>,
) -> Result<Json<TransactionStatus>> {
    tracing::info!("Fetching transaction status for signature: {}", signature);

    // Verify transaction belongs to user
    let tx_record = sqlx::query!(
        "SELECT * FROM blockchain_transactions WHERE signature = $1 AND user_id = $2",
        signature,
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch transaction: {}", e);
        ApiError::Database(e)
    })?
    .ok_or_else(|| ApiError::NotFound("Transaction not found".to_string()))?;

    // Fetch real status from blockchain
    let sig = Signature::from_str(&signature)
        .map_err(|_| ApiError::BadRequest("Invalid signature format".to_string()))?;

    let on_chain_status = state
        .blockchain_service
        .get_signature_status(&sig)
        .await
        .unwrap_or(None);

    let status_str = match on_chain_status {
        Some(true) => "confirmed",
        Some(false) => "failed",
        None => "pending", // or unknown if too old
    };

    // Construct response
    let transaction_status = TransactionStatus {
        signature: signature.clone(),
        status: status_str.to_string(),
        block_height: None, // Would need fetch_transaction to get this
        confirmation_status: if on_chain_status == Some(true) {
            "finalized".to_string()
        } else {
            "processed".to_string()
        },
        fee: tx_record
            .fee
            .map(|bd| {
                use std::str::FromStr;
                rust_decimal::Decimal::from_str(&bd.to_string()).unwrap_or_default()
            })
            .unwrap_or_default(),
        compute_units_consumed: tx_record.compute_units_consumed.map(|cu| cu as u32),
        logs: vec![], // Would need fetch_transaction to get logs
        program_interactions: vec![],
    };

    Ok(Json(transaction_status))
}

/// Interact with a specific smart contract program
/// POST /api/blockchain/programs/:name
#[utoipa::path(
    post,
    path = "/api/blockchain/programs/{name}",
    tag = "blockchain",
    request_body = ProgramInteractionRequest,
    security(("bearer_auth" = [])),
    params(
        ("name" = String, Path, description = "Program name (registry, trading, energy-token, oracle, governance)")
    ),
    responses(
        (status = 200, description = "Program interaction submitted", body = TransactionResponse),
        (status = 400, description = "Invalid program name or request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn interact_with_program(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(program_name): Path<String>,
    Json(payload): Json<ProgramInteractionRequest>,
) -> Result<Json<TransactionResponse>> {
    tracing::info!(
        "Program interaction request for: {} by user: {}",
        program_name,
        user.0.sub
    );

    // Validate program name
    let valid_programs = vec![
        "registry",
        "trading",
        "energy-token",
        "oracle",
        "governance",
    ];
    if !valid_programs.contains(&program_name.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Invalid program name: {}",
            program_name
        )));
    }

    // Validate instruction
    if payload.instruction.is_empty() {
        return Err(ApiError::BadRequest(
            "Instruction cannot be empty".to_string(),
        ));
    }

    // Simulate program interaction
    let signature = format!(
        "prog_{}_{}",
        program_name,
        Uuid::new_v4().to_string().replace('-', "")
    );

    // Log program interaction
    sqlx::query!(
        r#"
        INSERT INTO blockchain_transactions 
        (signature, user_id, program_id, instruction_name, status, compute_units_consumed, submitted_at)
        VALUES ($1, $2, $3, $4, 'Pending', $5, NOW())
        "#,
        signature,
        user.0.sub,
        program_name,
        payload.instruction.clone(),
        payload.compute_units.unwrap_or(10000) as i32
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to log program interaction: {}", e);
        ApiError::Database(e)
    })?;

    let response = TransactionResponse {
        signature: signature.clone(),
        status: "pending".to_string(),
        submitted_at: Utc::now(),
        estimated_confirmation_time: 15,
    };

    tracing::info!("Program interaction submitted: {}", signature);
    Ok(Json(response))
}

/// Get account information for a given address
/// GET /api/blockchain/accounts/:address
#[utoipa::path(
    get,
    path = "/api/blockchain/accounts/{address}",
    tag = "blockchain",
    security(("bearer_auth" = [])),
    params(
        ("address" = String, Path, description = "Solana account address (base58)")
    ),
    responses(
        (status = 200, description = "Account information", body = AccountInfo),
        (status = 400, description = "Invalid address format"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_account_info(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(address): Path<String>,
) -> Result<Json<AccountInfo>> {
    tracing::info!(
        "Fetching account info for address: {} by user: {}",
        address,
        user.0.sub
    );

    // Validate address format
    let pubkey = Pubkey::from_str(&address)
        .map_err(|_| ApiError::BadRequest("Invalid address format".to_string()))?;

    // Fetch real account info
    let balance_lamports = state
        .blockchain_service
        .get_balance(&pubkey)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch balance: {}", e)))?;

    let data = state
        .blockchain_service
        .get_account_data(&pubkey)
        .await
        .unwrap_or_default();

    let owner = "11111111111111111111111111111111".to_string(); // Default to system program if unknown
    // In a full implementation we would fetch the full Account object to get owner, executable, etc.

    let account_info = AccountInfo {
        address: address.clone(),
        balance: rust_decimal::Decimal::from(balance_lamports)
            / rust_decimal::Decimal::from(1_000_000_000),
        executable: false, // Placeholder
        owner,
        rent_epoch: 0,
        data_length: data.len(),
    };

    Ok(Json(account_info))
}

/// Get current network status
/// GET /api/blockchain/network
#[utoipa::path(
    get,
    path = "/api/blockchain/network",
    tag = "blockchain",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current blockchain network status", body = NetworkStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_network_status(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<NetworkStatus>> {
    tracing::info!("Fetching network status");

    let slot = state
        .blockchain_service
        .get_slot()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get slot: {}", e)))?;

    let is_healthy = state
        .blockchain_service
        .health_check()
        .await
        .unwrap_or(false);

    let network_status = NetworkStatus {
        cluster: state.blockchain_service.cluster().to_string(),
        block_height: slot,
        block_time: Utc::now(),
        tps: 0.0, // Would need more complex calculation
        health: if is_healthy {
            "ok".to_string()
        } else {
            "degraded".to_string()
        },
        version: "unknown".to_string(),
    };

    Ok(Json(network_status))
}
