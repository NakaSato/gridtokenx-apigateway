//! User registration handlers

use axum::{extract::State, http::StatusCode, response::Json};

use tracing::info;
use uuid::Uuid;
use validator::Validate;

use crate::auth::password::PasswordService;
use crate::error::{ApiError, Result};
use crate::AppState;

use super::types::{log_user_activity, RegisterRequest, RegisterResponse};

/// Enhanced user registration with email verification
#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered successfully", body = RegisterResponse),
        (status = 400, description = "Validation error or user already exists"),
        (status = 500, description = "Failed to send verification email")
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>)> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // All new users default to "user" role
    let default_role = "user";

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

    // Hash password
    let password_hash = PasswordService::hash_password(&request.password)?;

    // Create user with enhanced fields (email_verified = false by default)
    // wallet_address is NULL initially - assigned after email verification
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role,
                           first_name, last_name, is_active,
                           email_verified, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5::user_role, $6, $7, true, false, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(&request.username)
    .bind(&request.email)
    .bind(&password_hash)
    .bind(default_role)
    .bind(&request.first_name)
    .bind(&request.last_name)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to create user: {}", e)))?;

    // Generate verification token
    let token = crate::services::TokenService::generate_verification_token();
    // Log token for testing purposes
    info!(
        "Verification token generated for {}: {}",
        request.email, token
    );

    let token_hash = crate::services::TokenService::hash_token(&token);

    // Calculate expiration time from config
    let expiry_hours = state.config.email.verification_expiry_hours;
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(expiry_hours);

    // Store hashed token in database
    sqlx::query(
        "UPDATE users SET
         email_verification_token = $1,
         email_verification_sent_at = NOW(),
         email_verification_expires_at = $2
         WHERE id = $3",
    )
    .bind(&token_hash)
    .bind(expires_at)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to store verification token: {}", e)))?;

    // Send verification email if email service is available
    let email_sent = if let Some(email_service) = &state.email_service {
        match email_service
            .send_verification_email(&request.email, &token, &request.username)
            .await
        {
            Ok(_) => {
                // Log successful email send
                let _ = log_user_activity(
                    &state.db,
                    user_id,
                    "email_verification_sent".to_string(),
                    Some(serde_json::json!({
                        "email": request.email,
                        "expires_at": expires_at
                    })),
                    None,
                    None,
                )
                .await;
                true
            }
            Err(e) => {
                // Log failed email send but don't fail registration
                use tracing::error;
                error!("Failed to send verification email: {}", e);
                let _ = log_user_activity(
                    &state.db,
                    user_id,
                    "email_verification_send_failed".to_string(),
                    Some(serde_json::json!({
                        "email": request.email,
                        "error": e.to_string()
                    })),
                    None,
                    None,
                )
                .await;
                false
            }
        }
    } else {
        tracing::warn!("Email service not configured, skipping verification email");
        false
    };

    // Log user registration activity
    let _ = log_user_activity(
        &state.db,
        user_id,
        "user_registered".to_string(),
        Some(serde_json::json!({
            "role": default_role,
            "email_verification_sent": email_sent
        })),
        None,
        None,
    )
    .await;

    // Return registration response (NO JWT - user must verify email first)
    let response = RegisterResponse {
        message: if email_sent {
            "Registration successful! Please check your email to verify your account.".to_string()
        } else {
            "Registration successful! Email verification is pending. Please contact support if you don't receive the email.".to_string()
        },
        email_verification_sent: email_sent,
    };

    Ok((StatusCode::CREATED, Json(response)))
}
