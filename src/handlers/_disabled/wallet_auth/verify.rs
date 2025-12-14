use crate::services::wallet::service::WalletService;
use axum::{extract::State, response::Json};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use validator::Validate;

use super::types::{WalletLoginResponse, UserWalletInfo};
use crate::auth::{Claims, SecureUserInfo};
use crate::error::{ApiError, Result};
use crate::AppState;
use serde::Deserialize;
use utoipa::ToSchema;

/// Request for wallet signature verification
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct VerifyWalletRequest {
    #[validate(length(min = 32, max = 44))]
    pub wallet_address: String,
    
    #[validate(length(min = 1))]
    pub signature: String,
    
    #[validate(length(min = 1))]
    pub message: String,
    
    /// Timestamp included in the message to prevent replay attacks
    pub timestamp: i64,
}

/// Verify wallet signature and login
#[utoipa::path(
    post,
    path = "/api/auth/wallet/verify",
    tag = "Authentication",
    request_body = VerifyWalletRequest,
    responses(
        (status = 200, description = "Signature verified and logged in", body = WalletLoginResponse),
        (status = 401, description = "Invalid signature or timestamp expired"),
        (status = 404, description = "User not found with this wallet address"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn verify_wallet_signature(
    State(state): State<AppState>,
    Json(request): Json<VerifyWalletRequest>,
) -> Result<Json<WalletLoginResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // 1. Check timestamp freshness (prevent replay attacks)
    // Allow for 5 minutes drift/delay
    let now = chrono::Utc::now().timestamp();
    if (now - request.timestamp).abs() > 300 {
        return Err(ApiError::Unauthorized("Message timestamp expired".to_string()));
    }

    // 2. Verify expected message format
    // Expected format: "Sign in to GridTokenX. Timestamp: <timestamp>"
    let expected_message = format!("Sign in to GridTokenX. Timestamp: {}", request.timestamp);
    if request.message != expected_message {
        return Err(ApiError::Unauthorized("Invalid message format".to_string()));
    }

    // 3. Verify signature
    let pubkey = Pubkey::from_str(&request.wallet_address)
        .map_err(|_| ApiError::BadRequest("Invalid wallet address format".to_string()))?;
        
    let signature_bytes = bs58::decode(&request.signature)
        .into_vec()
        .map_err(|_| ApiError::BadRequest("Invalid signature format".to_string()))?;
        
    use solana_sdk::signature::Signature;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| ApiError::BadRequest("Invalid signature length".to_string()))?;
        
    if !signature.verify(pubkey.as_ref(), request.message.as_bytes()) {
        return Err(ApiError::Unauthorized("Invalid signature".to_string()));
    }

    // 4. Find user by wallet address
    use sqlx::Row;
    let row_opt: Option<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, username, email, role::text as role,
                first_name, last_name, wallet_address, is_active
         FROM users WHERE wallet_address = $1",
    )
    .bind(&request.wallet_address)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let user = match row_opt {
        Some(row) => {
            struct DbUser {
                id: uuid::Uuid,
                username: String,
                email: String,
                role: Option<String>,
                is_active: Option<bool>,
            }
            DbUser {
                id: row.get::<uuid::Uuid, _>("id"),
                username: row.get::<String, _>("username"),
                email: row.get::<String, _>("email"),
                role: row.get::<Option<String>, _>("role"),
                is_active: row.get::<Option<bool>, _>("is_active"),
            }
        },
        None => return Err(ApiError::NotFound("No user found with this wallet address. Please register first.".to_string())),
    };
    
    // Check if user is active
    if user.is_active == Some(false) || !user.is_active.unwrap_or(true) {
        return Err(ApiError::Unauthorized("Account is deactivated".to_string()));
    }

    // 5. Get wallet info
    let wallet_service = WalletService::new(&state.config.solana_rpc_url);
    let balance_lamports = wallet_service.get_balance(&pubkey).await.ok();
    let balance_sol = balance_lamports.map(crate::services::wallet::service::lamports_to_sol);

    let wallet_info = Some(UserWalletInfo {
        address: request.wallet_address.clone(),
        balance_lamports,
        balance_sol,
    });

    // 6. Generate JWT
    let claims = Claims::new(
        user.id,
        user.username.clone(),
        user.role.clone().unwrap_or_default(),
    );

    let access_token = state.jwt_service.encode_token(&claims)?;

    let response = WalletLoginResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60,
        user: SecureUserInfo {
            username: user.username,
            email: user.email,
            role: user.role.unwrap_or_default(),
            blockchain_registered: true,
        },
        wallet_info,
    };

    Ok(Json(response))
}
