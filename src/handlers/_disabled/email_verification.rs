use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    services::auth::{ResendVerificationResult, VerifyEmailResult},
    AppState,
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

// ============================================================================
// Verify Email Handler
// ============================================================================

/// Verify a user's email address using the verification token
///
/// GET /api/auth/verify-email?token={token}
///
/// This endpoint delegates verification to AuthService.
#[utoipa::path(
    get,
    path = "/api/auth/verify-email",
    tag = "auth",
    params(
        ("token" = String, Query, description = "Email verification token")
    ),
    responses(
        (status = 200, description = "Email verified successfully", body = VerifyEmailResult),
        (status = 400, description = "Invalid or expired token"),
        (status = 410, description = "Token expired")
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<VerifyEmailQuery>,
) -> Result<Json<VerifyEmailResult>, ApiError> {
    let result = state.auth.verify_email(&params.token).await?;
    Ok(Json(result))
}

// ============================================================================
// Resend Verification Email Handler
// ============================================================================

/// Resend verification email to a user
///
/// POST /api/auth/resend-verification
///
/// This endpoint delegates logic to AuthService.
#[utoipa::path(
    post,
    path = "/api/auth/resend-verification",
    tag = "auth",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Verification email sent successfully", body = ResendVerificationResult),
        (status = 400, description = "Invalid email or already verified"),
        (status = 404, description = "User not found"),
        (status = 429, description = "Too many requests - rate limit exceeded", body = ResendVerificationResult)
    )
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<(StatusCode, Json<ResendVerificationResult>), ApiError> {
    let result = state
        .auth
        .resend_verification(&payload.email)
        .await?;

    // Check if result indicates rate limiting
    if let Some(ref status) = result.status {
        if status == "rate_limited" {
            return Ok((StatusCode::TOO_MANY_REQUESTS, Json(result)));
        }
    }

    Ok((StatusCode::OK, Json(result)))
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
}
