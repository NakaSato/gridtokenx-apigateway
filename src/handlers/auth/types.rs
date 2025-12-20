//! Auth Types Module
//!
//! All request/response types for authentication, users, meters, tokens, and status.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

// ============================================================================
// Database Models
// ============================================================================

/// User row from database
#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub wallet_address: Option<String>,
}

// ============================================================================
// Auth Types
// ============================================================================

/// Login Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Auth Response (Token)
#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

// ============================================================================
// User Types
// ============================================================================

/// Registration Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegistrationRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
}

/// Registration Response
#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrationResponse {
    pub message: String,
    pub email_verification_sent: bool,
    pub auth: Option<AuthResponse>,
}

/// User Response
#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub first_name: String,
    pub last_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_address: Option<String>,
}

/// Email Verification Request
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct VerifyEmailRequest {
    pub token: String,
}

/// Email Verification Response
#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyEmailResponse {
    pub success: bool,
    pub message: String,
}

/// Resend Email Verification
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResendVerificationRequest {
    pub email: String,
}

/// Forgot Password Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// Reset Password Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// Change Password Request (for authenticated users)
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// ============================================================================
// Meter Types
// ============================================================================

/// Meter Response
#[derive(Debug, Serialize, ToSchema)]
pub struct MeterResponse {
    pub id: Uuid,
    pub serial_number: String,
    pub meter_type: String,
    pub location: String,
    pub is_verified: bool,
    pub wallet_address: String,
}

/// Meter Registration Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMeterRequest {
    pub serial_number: String,
    pub meter_type: Option<String>,
    pub location: Option<String>,
}

/// Meter Registration Response
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterMeterResponse {
    pub success: bool,
    pub message: String,
    pub meter: Option<MeterResponse>,
}

/// Verify Meter Request (Admin/System)
#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyMeterRequest {
    pub serial_number: String,
}

/// Query params for filtering meters
#[derive(Debug, Deserialize, IntoParams)]
pub struct MeterFilterParams {
    pub status: Option<String>,
}

/// Update meter status request
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMeterStatusRequest {
    pub status: String,  // "verified", "pending", "inactive"
}

/// Create reading request for v1 API
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReadingRequest {
    pub kwh: f64,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub wallet_address: Option<String>,
}

/// Create reading response
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateReadingResponse {
    pub id: Uuid,
    pub serial_number: String,
    pub kwh: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub minted: bool,
    pub tx_signature: Option<String>,
    pub message: String,
}

/// Reading Response Object
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub meter_serial: String,
    pub kwh: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub minted: bool,
    pub tx_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Query Params for Readings
#[derive(Debug, Deserialize, IntoParams)]
pub struct ReadingFilterParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub serial_number: Option<String>,
}

// ============================================================================
// Token/Wallet Types
// ============================================================================

/// Token Balance Response
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenBalanceResponse {
    pub wallet_address: String,
    pub token_balance: String,
    pub token_balance_raw: f64,
    pub balance_sol: f64,
    pub decimals: u8,
    pub token_mint: String,
    pub token_account: String,
}

// ============================================================================
// Status Types
// ============================================================================

/// System status response
#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub uptime: String,
}
