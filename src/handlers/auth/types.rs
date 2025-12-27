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
    /// Username or Email address of the user
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
#[derive(Debug, Serialize, ToSchema, Default)]
pub struct VerifyEmailResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthResponse>,
}

impl VerifyEmailResponse {
    /// Create a simple success/failure response without auth data
    pub fn simple(success: bool, message: impl Into<String>) -> Self {
        Self {
            success,
            message: message.into(),
            wallet_address: None,
            auth: None,
        }
    }
    
    /// Create a success response with auth data for auto-login
    pub fn with_auth(message: impl Into<String>, wallet_address: Option<String>, auth: Option<AuthResponse>) -> Self {
        Self {
            success: true,
            message: message.into(),
            wallet_address,
            auth,
        }
    }
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

/// Meter Response (for authenticated endpoints)
#[derive(Debug, Serialize, ToSchema)]
pub struct MeterResponse {
    pub id: Uuid,
    pub serial_number: String,
    pub meter_type: String,
    pub location: String,
    pub is_verified: bool,
    pub wallet_address: String,
    /// Latitude coordinate for map display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    /// Longitude coordinate for map display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

/// Public Meter Response (for unauthenticated public API)
/// 
/// Contains only safe-to-expose information for map display.
/// Excludes sensitive data like wallet addresses, serial numbers, and internal IDs.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicMeterResponse {
    /// Display name/location of the meter
    pub location: String,
    /// Type of meter (e.g., "Solar_Prosumer", "Consumer_Only")
    pub meter_type: String,
    /// Whether the meter is verified
    pub is_verified: bool,
    /// Latitude coordinate for map display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    /// Longitude coordinate for map display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    /// Latest energy generation reading (kW)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_generation: Option<f64>,
    /// Latest energy consumption reading (kW)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_consumption: Option<f64>,
}

/// Meter Registration Request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMeterRequest {
    pub serial_number: String,
    pub meter_type: Option<String>,
    pub location: Option<String>,
    /// Latitude coordinate for map display
    pub latitude: Option<f64>,
    /// Longitude coordinate for map display
    pub longitude: Option<f64>,
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

/// Create reading request for v1 API with full telemetry support
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReadingRequest {
    // Required fields
    pub kwh: f64,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub wallet_address: Option<String>,
    
    // Core Meter Identity
    pub meter_id: Option<String>,
    pub meter_type: Option<String>,
    
    // Energy Data (kWh)
    pub energy_generated: Option<f64>,
    pub energy_consumed: Option<f64>,
    pub surplus_energy: Option<f64>,
    pub deficit_energy: Option<f64>,
    
    // Electrical Parameters
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub power_factor: Option<f64>,
    pub frequency: Option<f64>,
    pub temperature: Option<f64>,
    
    // Location (GPS)
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    
    // Battery & Environmental
    pub battery_level: Option<f64>,
    pub weather_condition: Option<String>,
    
    // Trading & Certification
    pub rec_eligible: Option<bool>,
    pub carbon_offset: Option<f64>,
    pub max_sell_price: Option<f64>,
    pub max_buy_price: Option<f64>,
    
    // Security
    pub meter_signature: Option<String>,
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

/// Meter Stats Response
#[derive(Debug, Serialize, Default, ToSchema)]
pub struct MeterStats {
    pub total_produced: f64,
    pub total_consumed: f64,
    pub last_reading_time: Option<chrono::DateTime<chrono::Utc>>,
    pub total_minted: f64,
    pub total_minted_count: i64,
    pub pending_mint: f64,
    pub pending_mint_count: i64,
}

/// Query Params for Create Reading endpoint
#[derive(Debug, Deserialize, IntoParams, Default)]
pub struct CreateReadingParams {
    /// If false, skip auto-minting and just record the reading. Default: true
    pub auto_mint: Option<bool>,
    /// Timeout in seconds for blockchain operations. Default: 30
    pub timeout_secs: Option<u64>,
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
