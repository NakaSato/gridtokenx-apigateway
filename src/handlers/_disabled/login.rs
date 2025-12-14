use axum::{extract::State, http::HeaderMap, response::Json};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signer;
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::password::PasswordService;
use crate::auth::{Claims, SecureAuthResponse, SecureUserInfo};
use crate::error::{ApiError, Result};
use crate::services::AuditEvent;
use crate::utils::{extract_ip_address, extract_user_agent};
use crate::AppState;

use super::UserRow;

/// Login request
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 50))]
    #[schema(example = "john_doe", min_length = 3, max_length = 50)]
    pub username: String,

    #[validate(length(min = 8, max = 128))]
    #[schema(example = "SecurePassword123!", min_length = 8, max_length = 128)]
    pub password: String,
}

/// Login handler
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = SecureAuthResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "Email not verified"),
        (status = 400, description = "Validation error")
    )
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Json<SecureAuthResponse>> {
    // Extract IP and user-agent for audit logging
    let ip_address = extract_ip_address(&headers);
    let user_agent = extract_user_agent(&headers);

    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Find user by username
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email, password_hash, role::text as role,
                first_name, last_name, wallet_address, blockchain_registered,
                is_active, email_verified, created_at, updated_at
         FROM users 
         WHERE username = $1 AND is_active = true",
    )
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    let user = match user {
        Some(u) => u,
        None => {
            // Log failed login attempt
            state.audit_logger.log_async(AuditEvent::LoginFailed {
                email: request.username.clone(),
                ip: ip_address,
                reason: "User not found".to_string(),
                user_agent,
            });
            return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
        }
    };

    // Verify password
    let password_valid = PasswordService::verify_password(&request.password, &user.password_hash)?;
    if !password_valid {
        // Log failed login attempt
        state.audit_logger.log_async(AuditEvent::LoginFailed {
            email: user.email.clone(),
            ip: ip_address,
            reason: "Invalid password".to_string(),
            user_agent,
        });
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    // Check email verification if required (bypass in test mode)
    if state.config.email.verification_required && !user.email_verified && !state.config.test_mode {
        // Log failed login due to unverified email
        state.audit_logger.log_async(AuditEvent::LoginFailed {
            email: user.email.clone(),
            ip: ip_address.clone(),
            reason: "Email not verified".to_string(),
            user_agent: user_agent.clone(),
        });
        return Err(ApiError::Forbidden(
            "Email not verified. Please check your email for verification link.".to_string(),
        ));
    }

    // Check for wallet and create if missing (First Login)
    let mut wallet_address_resp = user.wallet_address.clone();

    if user.wallet_address.is_none() {
        // Generate new keypair
        let keypair = crate::services::WalletService::create_keypair();
        let pubkey = keypair.pubkey().to_string();

        // Encrypt private key with master secret (consistent with trading handlers)
        // This allows the system to decrypt keys for blockchain operations without user password
        let master_secret =
            std::env::var("WALLET_MASTER_SECRET").unwrap_or_else(|_| "dev-secret-key".to_string());
        let (encrypted_key_b64, salt_b64, iv_b64) =
            crate::services::WalletService::encrypt_private_key(
                &master_secret,
                &keypair.to_bytes(),
            )
            .map_err(|e| ApiError::Internal(format!("Failed to secure wallet: {}", e)))?;

        // Decode base64 to bytes for BYTEA storage
        use base64::{engine::general_purpose, Engine as _};
        let encrypted_key = general_purpose::STANDARD
            .decode(&encrypted_key_b64)
            .map_err(|e| ApiError::Internal(format!("Failed to decode encrypted key: {}", e)))?;
        let salt = general_purpose::STANDARD
            .decode(&salt_b64)
            .map_err(|e| ApiError::Internal(format!("Failed to decode salt: {}", e)))?;
        let iv = general_purpose::STANDARD
            .decode(&iv_b64)
            .map_err(|e| ApiError::Internal(format!("Failed to decode IV: {}", e)))?;

        // Update user with wallet info
        sqlx::query!(
            "UPDATE users 
             SET wallet_address = $1, 
                 encrypted_private_key = $2, 
                 wallet_salt = $3, 
                 encryption_iv = $4,
                 updated_at = NOW()
             WHERE id = $5",
            pubkey,
            &encrypted_key[..],
            &salt[..],
            &iv[..],
            user.id
        )
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to save wallet: {}", e)))?;

        // Log wallet creation
        state
            .audit_logger
            .log_async(AuditEvent::BlockchainRegistration {
                user_id: user.id,
                wallet_address: pubkey.clone(),
            });

        wallet_address_resp = Some(pubkey);
    }

    // Create JWT claims
    let claims = Claims::new(user.id, user.username.clone(), user.role.clone());

    // Generate token
    let access_token = state.jwt_service.encode_token(&claims)?;

    // Update last login
    let _ = sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await;

    // Log successful login
    state.audit_logger.log_async(AuditEvent::UserLogin {
        user_id: user.id,
        ip: ip_address,
        user_agent,
    });

    let response = SecureAuthResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 60 * 60, // 24 hours in seconds
        user: SecureUserInfo {
            username: user.username,
            email: user.email,
            role: user.role,
            blockchain_registered: wallet_address_resp.is_some(),
        },
    };

    Ok(Json(response))
}
