use crate::services::wallet::service::WalletService;
use axum::{extract::State, response::Json};
use solana_sdk::pubkey::Pubkey; // Added missing import
use solana_sdk::signer::Signer;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

use super::types::{DevWalletInfo, WalletRegistrationRequest, WalletRegistrationResponse};
use crate::auth::password::PasswordService;
use crate::auth::{Claims, SecureUserInfo};
use crate::error::{ApiError, Result};
use crate::AppState;

/// Enhanced registration with automatic wallet creation
#[utoipa::path(
    post,
    path = "/api/auth/register-with-wallet",
    tag = "Authentication",
    request_body = WalletRegistrationRequest,
    responses(
        (status = 200, description = "User registered successfully with optional wallet", body = WalletRegistrationResponse),
        (status = 400, description = "Invalid registration data or user already exists"),
        (status = 500, description = "Internal server error during registration or wallet creation")
    )
)]
pub async fn register_with_wallet(
    State(state): State<AppState>,
    Json(request): Json<WalletRegistrationRequest>,
) -> Result<Json<WalletRegistrationResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Validate role
    crate::auth::Role::from_str(&request.role)
        .map_err(|_| ApiError::BadRequest("Invalid role".to_string()))?;

    // Check if username already exists
    let existing_user = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE username = $1 OR email = $2",
    )
    .bind(&request.username)
    .bind(&request.email)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if existing_user > 0 {
        return Err(ApiError::BadRequest(
            "Username or email already exists".to_string(),
        ));
    }

    let mut wallet_address = request.wallet_address.clone();
    let mut wallet_info = None;
    let mut encrypted_private_key: Option<Vec<u8>> = None;
    let mut wallet_salt: Option<Vec<u8>> = None;
    let mut encryption_iv: Option<Vec<u8>> = None;

    // Create wallet if requested
    if request.create_wallet.unwrap_or(false) && wallet_address.is_none() {
        let wallet_service = WalletService::new(&state.config.solana_rpc_url);

        // Check if Solana RPC is available
        if wallet_service.health_check().await.is_err() {
            return Err(ApiError::Internal(
                "Solana RPC not available. Please ensure solana-test-validator is running."
                    .to_string(),
            ));
        }

        let keypair = WalletService::create_keypair();
        let pubkey = keypair.pubkey();

        // Store wallet address
        wallet_address = Some(pubkey.to_string());

        // For development: airdrop some SOL
        let airdrop_amount = request.airdrop_amount.unwrap_or(1.0);
        let airdrop_sig = if airdrop_amount > 0.0 {
            wallet_service
                .request_airdrop(&pubkey, airdrop_amount)
                .await
                .ok()
        } else {
            None
        };

        // Get balance after airdrop - wait for confirmation
        let mut balance_lamports = 0;
        for _ in 0..600 {
            balance_lamports = wallet_service.get_balance(&pubkey).await.unwrap_or(0);
            if balance_lamports > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
        let balance_sol = crate::services::wallet::service::lamports_to_sol(balance_lamports);

        // Encrypt private key for storage
        let (encrypted_key, salt, iv) = crate::utils::crypto::encrypt_to_bytes(
            &keypair.to_bytes(),
            &state.config.encryption_secret,
        )
        .map_err(|e| {
            tracing::error!("Failed to encrypt wallet: {}", e);
            ApiError::Internal("Failed to encrypt wallet".to_string())
        })?;

        encrypted_private_key = Some(encrypted_key);
        wallet_salt = Some(salt);
        encryption_iv = Some(iv);

        wallet_info = Some(DevWalletInfo {
            address: pubkey.to_string(),
            balance_lamports,
            balance_sol,
            private_key: bs58::encode(keypair.to_bytes()).into_string(),
            airdrop_signature: airdrop_sig.map(|s| s.to_string()),
            created_new: true,
        });

        // INTEGRATION: Register user on-chain
        let user_type: u8 = match request.role.to_lowercase().as_str() {
            "prosumer" => 0,
            "consumer" => 1,
            _ => 1, // Default to consumer
        };
        let location = "Unknown"; // Register user on-chain with their own keypair
        match state
            .blockchain_service
            .register_user_on_chain(&keypair, user_type, &location)
            .await
        {
            Ok(sig) => {
                tracing::info!("User registered on-chain successfully. Signature: {}", sig);

                // CRITICAL: Wait for transaction to be fully confirmed
                // This prevents AccountNotInitialized errors in subsequent operations
                // Increased to 4 seconds for more reliable confirmation on devnet
                tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;
            }
            Err(e) => {
                tracing::error!("Failed to register user on-chain: {}", e);
                // Non-blocking error - user can still use the platform
            }
        }
    } else if let Some(provided_address) = &wallet_address {
        // Validate provided wallet address
        if !WalletService::is_valid_address(provided_address) {
            return Err(ApiError::BadRequest(
                "Invalid Solana wallet address format".to_string(),
            ));
        }

        // Get balance for provided address
        let wallet_service = WalletService::new(&state.config.solana_rpc_url);
        if let Ok(pubkey) = Pubkey::from_str(provided_address) {
            let balance_lamports = wallet_service.get_balance(&pubkey).await.unwrap_or(0);
            let balance_sol = crate::services::wallet::service::lamports_to_sol(balance_lamports);

            wallet_info = Some(DevWalletInfo {
                address: provided_address.clone(),
                balance_lamports,
                balance_sol,
                private_key: "Not available (user provided address)".to_string(),
                airdrop_signature: None,
                created_new: false,
            });
        }
    }

    // Hash password
    let password_hash = PasswordService::hash_password(&request.password)?;

    // Create user with enhanced fields
    let user_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let pool: &sqlx::PgPool = &state.db;
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role,
                            first_name, last_name, wallet_address, is_active, email_verified, created_at, updated_at,
                            encrypted_private_key, wallet_salt, encryption_iv)
         VALUES ($1, $2, $3, $4, $5::user_role, $6, $7, $8, true, true, $9, $10, $11, $12, $13)",
    )
    .bind(user_id)
    .bind(&request.username)
    .bind(&request.email)
    .bind(&password_hash)
    .bind(&request.role)
    .bind(&request.first_name)
    .bind(request.last_name.clone())
    .bind(wallet_address.clone())
    .bind(now)
    .bind(now)
    .bind(encrypted_private_key)
    .bind(wallet_salt)
    .bind(encryption_iv)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to create user: {}", e)))?;

    // Log wallet creation for security monitoring
    let _ = state
        .wallet_audit_logger
        .log_wallet_creation(user_id, wallet_address.as_deref().unwrap_or(""), None, None)
        .await;
    // Create JWT claims
    let claims = Claims::new(user_id, request.username.clone(), request.role.clone());

    // Generate token
    let access_token = state.jwt_service.encode_token(&claims)?;

    let response = WalletRegistrationResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60, // 24 hours in seconds
        user: SecureUserInfo {
            username: request.username,
            email: request.email,
            role: request.role,
            blockchain_registered: wallet_address.is_some(),
        },
        wallet_info,
    };

    Ok(Json(response))
}
