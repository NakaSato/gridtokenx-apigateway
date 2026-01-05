//! Registration Handlers Module
//!
//! User registration and verification email handlers.

use axum::{
    extract::State,
    Json,
};
use chrono::{Duration, Utc};
use tracing::info;
use uuid::Uuid;
use crate::AppState;
use crate::error::ApiError;
use crate::auth::password::PasswordService;
use super::types::{
    RegistrationRequest, RegistrationResponse, AuthResponse, UserResponse,
    ResendVerificationRequest, VerifyEmailResponse,
};

/// Register Handler - inserts user into database and sends verification email
#[utoipa::path(
    post,
    path = "/api/v1/users",
    request_body = RegistrationRequest,
    responses(
        (status = 200, description = "Registration successful", body = RegistrationResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "users"
)]
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegistrationRequest>,
) -> Result<Json<RegistrationResponse>, ApiError> {
    info!("📝 Registration for user: {} (email: {})", request.username, request.email);

    let id = Uuid::new_v4();
    
    // Hash password with bcrypt
    let password_hash = match PasswordService::hash_password(&request.password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("❌ Password hashing failed: {}", e);
            return Ok(Json(RegistrationResponse {
                message: format!("Registration failed: {}", e),
                email_verification_sent: false,
                auth: None,
            }));
        }
    };

    // Generate verification token
    let verification_token = Uuid::new_v4().to_string();
    let verification_expires_at = Utc::now() + Duration::hours(
        state.config.email.verification_expiry_hours
    );

    // Insert user into database with verification token
    // Note: Wallet columns are NULL until email verification
    let insert_result = sqlx::query(
        "INSERT INTO users (
            id, username, email, password_hash, role, first_name, last_name, 
            is_active, email_verified, blockchain_registered, 
            email_verification_token, email_verification_sent_at, email_verification_expires_at,
            created_at, updated_at
        )
         VALUES ($1, $2, $3, $4, 'user', $5, $6, true, false, false, $7, NOW(), $8, NOW(), NOW())"
    )
    .bind(id)
    .bind(&request.username)
    .bind(&request.email)
    .bind(&password_hash)
    .bind(&request.first_name)
    .bind(&request.last_name)
    .bind(&verification_token)
    .bind(verification_expires_at)
    .execute(&state.db)
    .await;

    if let Err(e) = insert_result {
        tracing::error!("❌ Database insert error: {}", e);
        
        return Ok(Json(RegistrationResponse {
            message: format!("Registration failed: {}", e),
            email_verification_sent: false,
            auth: None,
        }));
    }

    info!("✅ User created in database: {} (email: {}) (Pending Verification)", request.username, request.email);

    // Send verification email
    let email_sent = if let Some(ref email_service) = state.email_service {
        match email_service.send_verification_email(
            &request.email,
            &verification_token,
            &request.username,
        ).await {
            Ok(()) => {
                info!("📧 Verification email sent to {}", request.email);
                true
            }
            Err(e) => {
                tracing::error!("❌ Failed to send verification email: {}", e);
                false
            }
        }
    } else {
        info!("⚠️ Email service not configured, skipping verification email");
        false
    };

    // Generate JWT token for immediate login (email verification still required for full access)
    let claims = crate::auth::Claims::new(id, request.username.clone(), "user".to_string());
    let token = state.jwt_service.encode_token(&claims).unwrap_or_else(|_| {
        format!("token_{}_{}", request.username, id)
    });

    let user = UserResponse {
        id,
        username: request.username,
        email: request.email,
        role: "user".to_string(),
        first_name: request.first_name,
        last_name: request.last_name,
        wallet_address: None,
        balance: rust_decimal::Decimal::ZERO,
        locked_amount: rust_decimal::Decimal::ZERO,
        locked_energy: rust_decimal::Decimal::ZERO,
    };

    let auth = AuthResponse {
        access_token: token,
        expires_in: 86400,
        user,
    };

    let message = if email_sent {
        "Registration successful. Please check your email to verify your account.".to_string()
    } else {
        "Registration successful. Email verification may be delayed.".to_string()
    };

    Ok(Json(RegistrationResponse {
        message,
        email_verification_sent: email_sent,
        auth: Some(auth),
    }))
}

/// Resend verification email
#[utoipa::path(
    post,
    path = "/api/v1/auth/resend-verification",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Verification email sent", body = VerifyEmailResponse),
        (status = 404, description = "User not found")
    ),
    tag = "auth"
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(request): Json<ResendVerificationRequest>,
) -> Result<Json<VerifyEmailResponse>, ApiError> {
    info!("📧 Resend verification request for: {}", request.email);
    
    // Look up user by email
    let user_result = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, username, email_verified FROM users WHERE email = $1"
    )
    .bind(&request.email)
    .fetch_optional(&state.db)
    .await;

    let (user_id, username, email_verified) = match user_result {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok(Json(VerifyEmailResponse {
                success: false,
                message: "Email address not found.".to_string(),
                wallet_address: None,
                auth: None,
            }));
        }
        Err(e) => {
            tracing::error!("Database error looking up user: {}", e);
            return Ok(Json(VerifyEmailResponse {
                success: false,
                message: "An error occurred. Please try again.".to_string(),
                wallet_address: None,
                auth: None,
            }));
        }
    };

    // Check if already verified
    if email_verified {
        return Ok(Json(VerifyEmailResponse {
            success: true,
            message: "Email is already verified. You can login now.".to_string(),
            wallet_address: None,
            auth: None,
        }));
    }

    // Generate new verification token
    let verification_token = Uuid::new_v4().to_string();
    let verification_expires_at = Utc::now() + Duration::hours(
        state.config.email.verification_expiry_hours
    );

    // Update user with new token
    let update_result = sqlx::query(
        "UPDATE users SET 
            email_verification_token = $1, 
            email_verification_sent_at = NOW(), 
            email_verification_expires_at = $2 
         WHERE id = $3"
    )
    .bind(&verification_token)
    .bind(verification_expires_at)
    .bind(user_id)
    .execute(&state.db)
    .await;

    if let Err(e) = update_result {
        tracing::error!("Failed to update verification token: {}", e);
        return Ok(Json(VerifyEmailResponse {
            success: false,
            message: "Failed to generate new verification token.".to_string(),
            wallet_address: None,
            auth: None,
        }));
    }

    // Send verification email
    let email_sent = if let Some(ref email_service) = state.email_service {
        match email_service.send_verification_email(
            &request.email,
            &verification_token,
            &username,
        ).await {
            Ok(()) => {
                info!("📧 Verification email resent to {}", request.email);
                true
            }
            Err(e) => {
                tracing::error!("❌ Failed to resend verification email: {}", e);
                false
            }
        }
    } else {
        info!("⚠️ Email service not configured");
        false
    };

    if email_sent {
        Ok(Json(VerifyEmailResponse {
            success: true,
            message: format!("Verification email sent to {}. Please check your inbox.", request.email),
            wallet_address: None,
            auth: None,
        }))
    } else {
        Ok(Json(VerifyEmailResponse {
            success: false,
            message: "Failed to send verification email. Please try again later.".to_string(),
            wallet_address: None,
            auth: None,
        }))
    }
}
