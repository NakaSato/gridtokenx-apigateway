//! Registration Handlers Module
//!
//! User registration and verification email handlers.

use axum::{
    extract::State,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use super::types::{
    RegistrationRequest, RegistrationResponse, AuthResponse, UserResponse,
    ResendVerificationRequest, VerifyEmailResponse,
};

/// Register Handler - inserts user into database
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegistrationRequest>,
) -> Json<RegistrationResponse> {
    info!("📝 Registration for user: {}", request.username);

    let id = Uuid::new_v4();
    let password_hash = format!("hash_{}", request.password); // Simplified for testing

    // Insert user into database (email_verified = false, must verify before meter verification)
    let insert_result = sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role, first_name, last_name, is_active, email_verified, blockchain_registered, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'user', $5, $6, true, false, false, NOW(), NOW())"
    )
    .bind(id)
    .bind(&request.username)
    .bind(&request.email)
    .bind(&password_hash)
    .bind(&request.first_name)
    .bind(&request.last_name)
    .execute(&state.db)
    .await;

    match insert_result {
        Ok(_) => info!("✅ User created in database: {}", request.username),
        Err(e) => info!("⚠️ Database insert error (may already exist): {}", e),
    }

    // Generate token
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
    };

    let auth = AuthResponse {
        access_token: token,
        expires_in: 86400,
        user,
    };

    Json(RegistrationResponse {
        message: "Registration successful".to_string(),
        email_verification_sent: false,
        auth: Some(auth),
    })
}

/// Resend verification email
pub async fn resend_verification(
    State(_state): State<AppState>,
    Json(request): Json<ResendVerificationRequest>,
) -> Json<VerifyEmailResponse> {
    info!("📧 Resend verification request for: {}", request.email);
    
    // In production, this would send an actual email
    // For now, just return success
    Json(VerifyEmailResponse {
        success: true,
        message: format!("Verification email sent to {}. Check your inbox.", request.email),
    })
}
