use crate::services::wallet::service::WalletService;
use axum::{extract::State, response::Json};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use validator::Validate;

use super::types::{UserWalletInfo, WalletLoginResponse};
use crate::auth::password::PasswordService;
use crate::auth::{Claims, SecureUserInfo};
use crate::error::{ApiError, Result};
use crate::AppState;

/// Login with wallet information
#[utoipa::path(
    post,
    path = "/api/auth/login-with-wallet",
    tag = "Authentication",
    request_body = crate::handlers::auth::LoginRequest,
    responses(
        (status = 200, description = "Login successful with wallet information", body = WalletLoginResponse),
        (status = 401, description = "Invalid credentials or account inactive"),
        (status = 400, description = "Invalid request data"),
        (status = 500, description = "Internal server error during login")
    )
)]
pub async fn login_with_wallet(
    State(state): State<AppState>,
    Json(request): Json<crate::handlers::auth::LoginRequest>,
) -> Result<Json<WalletLoginResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Get user from database with proper type casting
    let user = sqlx::query!(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, is_active
         FROM users WHERE username = $1",
        request.username
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::Unauthorized("Invalid username or password".to_string()))?;

    // Check if user is active
    if user.is_active == Some(false) || !user.is_active.unwrap_or(true) {
        return Err(ApiError::Unauthorized("Account is deactivated".to_string()));
    }

    // Verify password
    if !PasswordService::verify_password(&request.password, &user.password_hash)? {
        return Err(ApiError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Get wallet information if user has a wallet
    let mut wallet_info = None;
    if let Some(wallet_addr) = &user.wallet_address {
        let wallet_service = WalletService::new(&state.config.solana_rpc_url);
        if let Ok(pubkey) = Pubkey::from_str(wallet_addr) {
            let balance_lamports = wallet_service.get_balance(&pubkey).await.ok();
            let balance_sol =
                balance_lamports.map(crate::services::wallet::service::lamports_to_sol);

            wallet_info = Some(UserWalletInfo {
                address: wallet_addr.to_string(),
                balance_lamports,
                balance_sol,
            });
        }
    }

    // Create JWT claims
    let claims = Claims::new(
        user.id,
        user.username.clone(),
        user.role.clone().unwrap_or_default(),
    );

    // Generate token
    let access_token = state.jwt_service.encode_token(&claims)?;

    let response = WalletLoginResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60,
        user: SecureUserInfo {
            username: user.username,
            email: user.email,
            role: user.role.unwrap_or_default(),
            blockchain_registered: user.wallet_address.is_some(),
        },
        wallet_info,
    };

    Ok(Json(response))
}
