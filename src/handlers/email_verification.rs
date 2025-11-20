use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signer;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{Claims, SecureAuthResponse, SecureUserInfo},
    error::ApiError,
    services::{AuditEvent, token_service::TokenService},
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailQuery {
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResendVerificationRequest {
    #[schema(example = "john.doe@example.com")]
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyEmailResponse {
    pub message: String,
    pub email_verified: bool,
    pub verified_at: String,
    pub wallet_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<SecureAuthResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResendVerificationResponse {
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_hours: Option<i64>,

    /// Status of the verification: "already_verified", "expired_resent", "sent", "rate_limited"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<i64>,
}

// ============================================================================
// Database Query Structs
// ============================================================================

#[derive(Debug)]
struct UserVerificationRecord {
    id: Uuid,
    email: Option<String>,
    username: Option<String>,
    email_verified: bool,
    email_verification_token: Option<String>,
    email_verification_expires_at: Option<chrono::DateTime<Utc>>,
    wallet_address: Option<String>,
    role: Option<String>,
}

// ============================================================================
// Verify Email Handler
// ============================================================================

/// Verify a user's email address using the verification token
///
/// GET /api/auth/verify-email?token={token}
///
/// This endpoint:
/// 1. Validates the token format and existence
/// 2. Checks if the token has expired
/// 3. Updates the user's email_verified status
/// 4. Invalidates the token (one-time use)
/// 5. Optionally returns a JWT token for immediate login
#[utoipa::path(
    get,
    path = "/api/auth/verify-email",
    tag = "auth",
    params(
        ("token" = String, Query, description = "Email verification token")
    ),
    responses(
        (status = 200, description = "Email verified successfully", body = VerifyEmailResponse),
        (status = 400, description = "Invalid or expired token"),
        (status = 410, description = "Token expired")
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<VerifyEmailQuery>,
) -> Result<Json<VerifyEmailResponse>, ApiError> {
    // Validate token format (Base58 encoded, reasonable length)
    if params.token.is_empty() || params.token.len() > 128 {
        return Err(ApiError::BadRequest("Invalid token format".to_string()));
    }

    // Hash the token to compare with database
    let hashed_token = TokenService::hash_token(&params.token);

    // Find user by verification token
    let user = sqlx::query_as!(
        UserVerificationRecord,
        r#"
        SELECT 
            id,
            email as "email?",
            username as "username?",
            email_verified,
            email_verification_token,
            email_verification_expires_at,
            wallet_address,
            role::text as "role?"
        FROM users
        WHERE email_verification_token = $1
        "#,
        hashed_token
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::BadRequest("Invalid or expired verification token".to_string()))?;

    // Validate user has email (required for verification)
    let user_email = user
        .email
        .as_ref()
        .ok_or_else(|| ApiError::Internal("User email is missing".to_string()))?;

    // Check if already verified
    if user.email_verified {
        return Err(ApiError::BadRequest("Email already verified".to_string()));
    }

    // Check if token has expired
    if let Some(expires_at) = user.email_verification_expires_at {
        if expires_at < Utc::now() {
            return Err(ApiError::BadRequest(
                "Verification token has expired. Please request a new one.".to_string(),
            ));
        }
    } else {
        return Err(ApiError::BadRequest(
            "Invalid verification token".to_string(),
        ));
    }

    // Generate a new Solana wallet for the user
    let keypair = crate::services::WalletService::create_keypair();
    let wallet_address = keypair.pubkey().to_string();

    tracing::info!(
        "Generated new wallet for user {}: {}",
        user.id,
        wallet_address
    );

    // Update user: set email_verified = true, clear token, set verified_at, and assign wallet
    let verified_at = Utc::now();
    sqlx::query!(
        r#"
        UPDATE users
        SET 
            email_verified = true,
            email_verification_token = NULL,
            email_verification_expires_at = NULL,
            email_verified_at = $1,
            wallet_address = $2
        WHERE id = $3
        "#,
        verified_at,
        wallet_address,
        user.id
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Log email verification to audit logs
    state
        .audit_logger
        .log_async(AuditEvent::EmailVerified { user_id: user.id });

    // Log wallet creation for audit trail
    state
        .audit_logger
        .log_async(AuditEvent::BlockchainRegistration {
            user_id: user.id,
            wallet_address: wallet_address.clone(),
        });

    // Send welcome email if email service is available
    if let Some(ref email_service) = state.email_service {
        let username = user.username.as_deref().unwrap_or("User");
        if let Err(e) = email_service.send_welcome_email(user_email, username).await {
            // Log error but don't fail the verification
            tracing::warn!("Failed to send welcome email: {}", e);
        }
    }

    // Create JWT token for immediate login (optional)
    let auth_response = if state.config.email.auto_login_after_verification {
        let username_str = user.username.clone().unwrap_or_else(|| "User".to_string());
        let role_str = user.role.clone().unwrap_or_else(|| "user".to_string());

        let claims = Claims::new(user.id, username_str.clone(), role_str.clone());
        let access_token = state.jwt_service.encode_token(&claims)?;

        Some(SecureAuthResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: state.config.jwt_expiration,
            user: SecureUserInfo {
                username: username_str,
                email: user_email.to_string(),
                role: role_str,
                blockchain_registered: true, // Wallet was just created
            },
        })
    } else {
        None
    };

    Ok(Json(VerifyEmailResponse {
        message: "Email verified successfully! Your Solana wallet has been created.".to_string(),
        email_verified: true,
        verified_at: verified_at.to_rfc3339(),
        wallet_address: wallet_address.clone(),
        auth: auth_response,
    }))
}

// ============================================================================
// Resend Verification Email Handler
// ============================================================================

/// Resend verification email to a user
///
/// POST /api/auth/resend-verification
///
/// This endpoint:
/// 1. Validates the user exists and is not already verified
/// 2. Checks rate limiting (prevents abuse)
/// 3. Generates a new verification token
/// 4. Sends a new verification email
/// 5. Updates the token expiration time
#[utoipa::path(
    post,
    path = "/api/auth/resend-verification",
    tag = "auth",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Verification email sent successfully", body = ResendVerificationResponse),
        (status = 400, description = "Invalid email or already verified"),
        (status = 404, description = "User not found"),
        (status = 429, description = "Too many requests - rate limit exceeded", body = ResendVerificationResponse)
    )
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<(StatusCode, Json<ResendVerificationResponse>), ApiError> {
    // Validate email format
    if payload.email.is_empty() || !payload.email.contains('@') {
        return Err(ApiError::BadRequest("Invalid email format".to_string()));
    }

    // Check if email verification is enabled
    if !state.config.email.verification_enabled {
        return Err(ApiError::BadRequest(
            "Email verification is not enabled".to_string(),
        ));
    }

    // Email service must be available
    let email_service = state
        .email_service
        .as_ref()
        .ok_or_else(|| ApiError::Configuration("Email service is not configured".to_string()))?;

    // Find user by email
    let user = sqlx::query_as!(
        UserVerificationRecord,
        r#"
        SELECT 
            id,
            email as "email?",
            username as "username?",
            email_verified,
            email_verification_token,
            email_verification_expires_at,
            wallet_address,
            role::text as "role?"
        FROM users
        WHERE email = $1
        "#,
        payload.email
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    // Validate user has email (required for verification)
    let user_email = user
        .email
        .as_ref()
        .ok_or_else(|| ApiError::Internal("User email is missing".to_string()))?;

    // Check if already verified - return success immediately
    if user.email_verified {
        let verified_at = Utc::now();
        return Ok((
            StatusCode::OK,
            Json(ResendVerificationResponse {
                message: "Email is already verified. No action needed.".to_string(),
                email: Some(user_email.to_string()),
                sent_at: Some(verified_at.to_rfc3339()),
                expires_in_hours: Some(0),
                status: Some("already_verified".to_string()),
                retry_after: None,
            }),
        ));
    }

    // Check if token has expired
    let is_token_expired = if let Some(expires_at) = user.email_verification_expires_at {
        expires_at < Utc::now()
    } else {
        // No expiry set means no token was sent yet, treat as expired
        true
    };

    // Rate limiting: Check if last email was sent within 10 seconds (to prevent spam)
    // BUT: Skip rate limiting if token has expired (allow immediate resend for expired tokens)
    if !is_token_expired {
        if let Some(expires_at) = user.email_verification_expires_at {
            // Calculate when the email was sent (24 hours before expiry)
            let sent_at = expires_at
                - chrono::Duration::hours(state.config.email.verification_expiry_hours as i64);
            let time_since_sent = Utc::now() - sent_at;

            // Allow resend after 10 seconds
            if time_since_sent < chrono::Duration::seconds(10) {
                let wait_seconds = 10 - time_since_sent.num_seconds();
                return Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ResendVerificationResponse {
                        message: format!(
                            "Rate limit exceeded. Please wait {} seconds before retrying",
                            wait_seconds
                        ),
                        email: None,
                        sent_at: None,
                        expires_in_hours: None,
                        status: None,
                        retry_after: None,
                    }),
                ));
            }
        }
    }

    // Generate new verification token
    let token = TokenService::generate_verification_token();
    let hashed_token = TokenService::hash_token(&token);

    // Update user with new token
    let sent_at = Utc::now();
    let expires_at =
        sent_at + chrono::Duration::hours(state.config.email.verification_expiry_hours as i64);

    sqlx::query!(
        r#"
        UPDATE users
        SET 
            email_verification_token = $1,
            email_verification_sent_at = $2,
            email_verification_expires_at = $3
        WHERE id = $4
        "#,
        hashed_token,
        sent_at,
        expires_at,
        user.id
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    // Send verification email
    let username = user.username.as_deref().unwrap_or("User");
    email_service
        .send_verification_email(user_email, username, &token)
        .await
        .map_err(|e| ApiError::ExternalService(format!("Failed to send email: {}", e)))?;

    // Determine response message and status based on whether token was expired
    let (message, status) = if is_token_expired {
        (
            "Your verification token has expired. A new verification email has been sent! Please check your inbox.".to_string(),
            Some("expired_resent".to_string())
        )
    } else {
        (
            "Verification email sent successfully! Please check your inbox.".to_string(),
            Some("sent".to_string()),
        )
    };

    Ok((
        StatusCode::OK,
        Json(ResendVerificationResponse {
            message,
            email: Some(user_email.to_string()),
            sent_at: Some(sent_at.to_rfc3339()),
            expires_in_hours: Some(state.config.email.verification_expiry_hours as i64),
            status,
            retry_after: None,
        }),
    ))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_email_query_deserialization() {
        let json = r#"{"token": "ABC123XYZ"}"#;
        let query: VerifyEmailQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.token, "ABC123XYZ");
    }

    #[test]
    fn test_resend_verification_request_deserialization() {
        let json = r#"{"email": "test@example.com"}"#;
        let req: ResendVerificationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.email, "test@example.com");
    }

    #[test]
    fn test_verify_email_response_serialization() {
        let response = VerifyEmailResponse {
            message: "Success".to_string(),
            email_verified: true,
            verified_at: "2024-01-15T10:30:00Z".to_string(),
            wallet_address: "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8".to_string(),
            auth: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Success"));
        assert!(json.contains("email_verified"));
        assert!(json.contains("wallet_address"));
    }

    #[test]
    fn test_resend_verification_response_serialization() {
        let response = ResendVerificationResponse {
            message: "Email sent".to_string(),
            email: Some("test@example.com".to_string()),
            sent_at: Some("2024-01-15T10:30:00Z".to_string()),
            expires_in_hours: Some(24),
            status: Some("sent".to_string()),
            retry_after: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Email sent"));
        assert!(json.contains("test@example.com"));
        assert!(json.contains("sent"));
    }
}
