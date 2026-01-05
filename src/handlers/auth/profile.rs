//! Profile Handlers Module
//!
//! User profile management handlers.

use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use super::types::{UserResponse, UserRow, UpdateWalletRequest};
use base64::{engine::general_purpose, Engine as _};
use solana_sdk::signature::{Keypair, Signer};
use crate::services::WalletService;

/// Profile Handler - fetches user from database by token
#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    responses(
        (status = 200, description = "User profile", body = UserResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "users"
)]
pub async fn profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<UserResponse> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    info!("👤 Profile request");

    // Try to decode token and get user from database
    if let Ok(claims) = state.jwt_service.decode_token(token) {
        let user_result = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, role::text as role, first_name, last_name, wallet_address, balance, locked_amount, locked_energy
             FROM users WHERE id = $1"
        )
        .bind(claims.sub)
        .fetch_optional(&state.db)
        .await;

        if let Ok(Some(user)) = user_result {
            info!("✅ Returning profile for: {} (email: {}) (from database)", user.username, user.email);
            return Json(UserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
                role: user.role,
                first_name: user.first_name.unwrap_or_default(),
                last_name: user.last_name.unwrap_or_default(),
                wallet_address: user.wallet_address,
                balance: user.balance.unwrap_or_default(),
                locked_amount: user.locked_amount.unwrap_or_default(),
                locked_energy: user.locked_energy.unwrap_or_default(),
            });
        }
    }

    // Fallback to guest
    info!("⚠️ Token invalid or user not found");
    Json(UserResponse {
        id: Uuid::new_v4(),
        username: "guest".to_string(),
        email: "guest@gridtokenx.com".to_string(),
        role: "user".to_string(),
        first_name: "Guest".to_string(),
        last_name: "User".to_string(),
        wallet_address: None,
        balance: rust_decimal::Decimal::ZERO,
        locked_amount: rust_decimal::Decimal::ZERO,
        locked_energy: rust_decimal::Decimal::ZERO,
    })
}

/// Update Wallet Handler
#[utoipa::path(
    post,
    path = "/api/v1/users/wallet",
    request_body = UpdateWalletRequest,
    responses(
        (status = 200, description = "Wallet updated", body = UserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal Server Error")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "users"
)]
pub async fn update_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateWalletRequest>,
) -> Result<Json<UserResponse>, crate::ApiError> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(crate::ApiError::Unauthorized("Missing token".to_string()))?;

    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    let claims = state.jwt_service.decode_token(token)
        .map_err(|_| crate::ApiError::Unauthorized("Invalid token".to_string()))?;

    info!("💼 Update wallet request for user: {}", claims.sub);

    // Update wallet in database
    let user = sqlx::query_as::<_, UserRow>(
        r#"
        UPDATE users 
        SET wallet_address = $1, blockchain_registered = true, updated_at = NOW() 
        WHERE id = $2
        RETURNING id, username, email, role::text as role, first_name, last_name, wallet_address, balance, locked_amount, locked_energy
        "#
    )
    .bind(&payload.wallet_address)
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update wallet: {}", e);
        crate::ApiError::Internal("Database error".to_string())
    })?;

    info!("✅ Wallet updated for user {}: {}", user.username, payload.wallet_address);

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        role: user.role,
        first_name: user.first_name.unwrap_or_default(),
        last_name: user.last_name.unwrap_or_default(),
        wallet_address: user.wallet_address,
                balance: user.balance.unwrap_or_default(),
                locked_amount: user.locked_amount.unwrap_or_default(),
                locked_energy: user.locked_energy.unwrap_or_default(),
    }))
}

/// Generate Wallet Handler
#[utoipa::path(
    post,
    path = "/api/v1/users/wallet/generate",
    responses(
        (status = 200, description = "Wallet generated", body = UserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal Server Error")
    ),
    security(
        ("jwt_token" = [])
    ),
    tag = "users"
)]
pub async fn generate_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, crate::ApiError> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(crate::ApiError::Unauthorized("Missing token".to_string()))?;

    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    let claims = state.jwt_service.decode_token(token)
        .map_err(|_| crate::ApiError::Unauthorized("Invalid token".to_string()))?;

    info!("🔑 Wallet generation request for user: {}", claims.sub);

    // Generate new keypair
    let new_keypair = Keypair::new();
    let pubkey = new_keypair.pubkey().to_string();
    let kp_bytes = new_keypair.to_bytes();
    
    let master_secret = &state.config.encryption_secret;

    let (enc_key_b64, salt_b64, iv_b64) = WalletService::encrypt_private_key(master_secret, &kp_bytes)
        .map_err(|e| {
            tracing::error!("Encryption failed: {}", e);
            crate::ApiError::Internal("Encryption failure".to_string())
        })?;

    let enc_key_bytes = general_purpose::STANDARD.decode(&enc_key_b64).unwrap_or_default();
    let salt_bytes = general_purpose::STANDARD.decode(&salt_b64).unwrap_or_default();
    let iv_bytes = general_purpose::STANDARD.decode(&iv_b64).unwrap_or_default();

    // Update DB
    let user = sqlx::query_as::<_, UserRow>(
        r#"
        UPDATE users 
        SET wallet_address = $1, encrypted_private_key = $2, wallet_salt = $3, encryption_iv = $4, blockchain_registered = true, updated_at = NOW() 
        WHERE id = $5
        RETURNING id, username, email, role::text as role, first_name, last_name, wallet_address, balance, locked_amount, locked_energy
        "#
    )
    .bind(&pubkey)
    .bind(&enc_key_bytes)
    .bind(&salt_bytes)
    .bind(&iv_bytes)
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update wallet in DB: {}", e);
        crate::ApiError::Internal("Database error".to_string())
    })?;

    info!("✅ New custodial wallet generated for user {}: {}", user.username, pubkey);

    // Request initial SOL airdrop (2.0 SOL) and wait for confirmation
    match state.wallet_service.request_airdrop(&new_keypair.pubkey(), 2.0).await {
        Ok(sig) => {
            info!("✅ Airdrop confirmed for {}: {}", pubkey, sig);
        }
        Err(e) => {
            tracing::error!("⚠️ Failed to request airdrop for {}: {}", pubkey, e);
        }
    }

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        role: user.role,
        first_name: user.first_name.unwrap_or_default(),
        last_name: user.last_name.unwrap_or_default(),
        wallet_address: user.wallet_address,
                balance: user.balance.unwrap_or_default(),
                locked_amount: user.locked_amount.unwrap_or_default(),
                locked_energy: user.locked_energy.unwrap_or_default(),
    }))
}
