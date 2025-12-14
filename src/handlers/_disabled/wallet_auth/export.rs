use axum::{extract::State, response::Json, Extension};
use solana_sdk::signature::Signer;
use solana_sdk::signer::keypair::Keypair;

use super::types::{ExportWalletRequest, ExportWalletResponse};
use crate::auth::password::PasswordService;
use crate::auth::Claims;
use crate::error::{ApiError, Result};
use crate::AppState;

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
    // 2. Check rate limit (1 export per hour)
    let rate_limit_check = sqlx::query!(
        r#"SELECT last_export_at as "last_export_at: chrono::DateTime<chrono::Utc>" FROM wallet_export_rate_limit WHERE user_id = $1"#,
        user.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if let Some(rate_limit) = rate_limit_check {
        let last_export = rate_limit.last_export_at;
        // Use signed_duration_since to get a TimeDelta/Duration
        let time_since_last_export = chrono::Utc::now().signed_duration_since(last_export);

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

    let encrypted_key = wallet_data.encrypted_private_key.ok_or_else(|| {
        tracing::error!("No encrypted private key found for user: {}", user.sub);
        ApiError::NotFound("No encrypted wallet found for this user".to_string())
    })?;

    let salt = wallet_data.wallet_salt.ok_or_else(|| {
        tracing::error!("No wallet salt found for user: {}", user.sub);
        ApiError::NotFound("Incomplete wallet data".to_string())
    })?;

    let iv = wallet_data.encryption_iv.ok_or_else(|| {
        tracing::error!("No encryption IV found for user: {}", user.sub);
        ApiError::NotFound("Incomplete wallet data".to_string())
    })?;

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
