use crate::services::wallet_service::WalletService;
use axum::{extract::State, response::Json, Extension};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signer, signer::keypair::Keypair};
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::auth::password::PasswordService;
use crate::auth::{Claims, SecureUserInfo};
use crate::error::{ApiError, Result};
use crate::AppState;

/// Enhanced registration request with wallet creation option
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct WalletRegistrationRequest {
    #[validate(length(min = 3, max = 50))]
    pub username: String,

    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8, max = 128))]
    pub password: String,

    #[validate(length(min = 1, max = 20))]
    pub role: String,

    #[validate(length(min = 1, max = 100))]
    pub first_name: String,

    #[validate(length(min = 1, max = 100))]
    pub last_name: String,

    /// Create a new Solana wallet for this user
    pub create_wallet: Option<bool>,

    /// Amount of SOL to airdrop (development only)
    pub airdrop_amount: Option<f64>,

    /// Optional manual wallet address (if not creating new one)
    #[validate(length(min = 32, max = 44))]
    pub wallet_address: Option<String>,
}

/// Response with wallet information for development
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletRegistrationResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: SecureUserInfo,
    pub wallet_info: Option<DevWalletInfo>,
}

/// Development wallet information (DO NOT USE IN PRODUCTION)
#[derive(Debug, Serialize, ToSchema)]
pub struct DevWalletInfo {
    pub address: String,
    pub balance_lamports: u64,
    pub balance_sol: f64,
    pub private_key: String, // Only for development!
    pub airdrop_signature: Option<String>,
    pub created_new: bool,
}

/// Login response with wallet information
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: SecureUserInfo,
    pub wallet_info: Option<UserWalletInfo>,
}

/// Request to export wallet private key
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ExportWalletRequest {
    /// User's current password for re-authentication
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

/// Response containing exported wallet private key
#[derive(Debug, Serialize, ToSchema)]
pub struct ExportWalletResponse {
    /// Private key in Base58 format
    pub private_key: String,
    /// Public key (wallet address)
    pub public_key: String,
    /// Security warning message
    pub warning: String,
}

/// User's wallet information (safe for production)
#[derive(Debug, Serialize, ToSchema)]
pub struct UserWalletInfo {
    pub address: String,
    pub balance_lamports: Option<u64>,
    pub balance_sol: Option<f64>,
}

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
        let balance_sol = crate::services::wallet_service::lamports_to_sol(balance_lamports);

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
            let balance_sol = crate::services::wallet_service::lamports_to_sol(balance_lamports);

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
    .bind(&request.last_name)
    .bind(&wallet_address)
    .bind(now)
    .bind(now)
    .bind(encrypted_private_key)
    .bind(wallet_salt)
    .bind(encryption_iv)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to create user: {}", e)))?;

    // Log wallet creation for security monitoring
    state
        .wallet_audit_logger
        .log_wallet_creation(user_id, wallet_address.as_deref().unwrap_or(""), None, None)
        .await
        .ok(); // Don't fail registration if audit logging fails

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
                balance_lamports.map(crate::services::wallet_service::lamports_to_sol);

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

/// Export wallet private key with security checks
///
/// This endpoint allows users to export their private key for backup purposes.
/// Security measures:
/// - Requires password re-authentication
/// - Rate limited to 1 export per hour
/// - All exports are audit logged
/// - Returns security warning
#[utoipa::path(
    post,
    path = "/api/wallet/export",
    tag = "Wallet",
    request_body = ExportWalletRequest,
    responses(
        (status = 200, description = "Wallet exported successfully", body = ExportWalletResponse),
        (status = 401, description = "Invalid password"),
        (status = 404, description = "No wallet found"),
        (status = 429, description = "Rate limit exceeded - 1 export per hour"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn export_wallet_handler(
    State(state): State<AppState>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<ExportWalletRequest>,
) -> Result<Json<ExportWalletResponse>> {
    tracing::info!("Wallet export requested for user: {}", user.sub);

    // 1. Verify password (re-authentication)
    let user_record = sqlx::query!("SELECT password_hash FROM users WHERE id = $1", user.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if !PasswordService::verify_password(&payload.password, &user_record.password_hash)? {
        tracing::warn!(
            "Failed wallet export attempt for user: {} - Invalid password",
            user.sub
        );
        return Err(ApiError::Unauthorized("Invalid password".to_string()));
    }

    // 2. Check rate limit (1 export per hour)
    let rate_limit_check = sqlx::query!(
        "SELECT last_export_at FROM wallet_export_rate_limit WHERE user_id = $1",
        user.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if let Some(rate_limit) = rate_limit_check {
        let time_since_last_export = chrono::Utc::now() - rate_limit.last_export_at;
        if time_since_last_export < chrono::TimeDelta::try_hours(1).unwrap() {
            let minutes_remaining = 60 - (time_since_last_export.num_seconds() / 60);
            tracing::warn!(
                "Rate limit exceeded for user: {} - {} minutes remaining",
                user.sub,
                minutes_remaining
            );
            return Err(ApiError::RateLimitExceeded(format!(
                "Rate limit exceeded. Please wait {} minutes before exporting again.",
                minutes_remaining
            )));
        }
    }

    // 3. Fetch encrypted wallet data
    let wallet_data = sqlx::query!(
        "SELECT encrypted_private_key, wallet_salt, encryption_iv, wallet_address 
         FROM users WHERE id = $1",
        user.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let (encrypted_key, salt, iv) = match (
        wallet_data.encrypted_private_key,
        wallet_data.wallet_salt,
        wallet_data.encryption_iv,
    ) {
        (Some(k), Some(s), Some(i)) => (k, s, i),
        _ => {
            tracing::error!("No encrypted wallet found for user: {}", user.sub);
            return Err(ApiError::NotFound(
                "No encrypted wallet found for this user".to_string(),
            ));
        }
    };

    // 4. Decrypt private key
    let decrypted_bytes = crate::utils::crypto::decrypt_bytes(
        &encrypted_key,
        &salt,
        &iv,
        &state.config.encryption_secret,
    )
    .map_err(|e| {
        tracing::error!("Failed to decrypt wallet for user: {} - {}", user.sub, e);
        ApiError::Internal("Failed to decrypt wallet".to_string())
    })?;

    // Convert decrypted bytes to keypair
    // The decrypted_bytes should be 64 bytes (32 for secret key + 32 for public key)
    if decrypted_bytes.len() != 64 {
        tracing::error!(
            "Invalid keypair length for user: {} - expected 64, got {}",
            user.sub,
            decrypted_bytes.len()
        );
        return Err(ApiError::Internal("Invalid wallet data length".to_string()));
    }

    // For solana-sdk 3.0, use new_from_array with 32-byte secret key
    let mut secret_key_bytes = [0u8; 32];
    secret_key_bytes.copy_from_slice(&decrypted_bytes[0..32]);
    let keypair = Keypair::new_from_array(secret_key_bytes);

    // 5. Update rate limit table
    sqlx::query!(
        "INSERT INTO wallet_export_rate_limit (user_id, last_export_at, export_count)
         VALUES ($1, NOW(), 1)
         ON CONFLICT (user_id) 
         DO UPDATE SET last_export_at = NOW(), export_count = wallet_export_rate_limit.export_count + 1",
        user.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to update rate limit: {}", e)))?;

    // Log successful export
    state
        .wallet_audit_logger
        .log_export(user.sub, None, None)
        .await
        .ok();

    tracing::info!("Wallet exported successfully for user: {}", user.sub);

    // 7. Return private key with security warning
    let response = ExportWalletResponse {
        private_key: bs58::encode(&keypair.to_bytes()).into_string(),
        public_key: keypair.pubkey().to_string(),
        warning: "⚠️ SECURITY WARNING: Store this private key securely. Anyone with access to this key can control your wallet and assets. Never share this key with anyone.".to_string(),
    };

    Ok(Json(response))
}
