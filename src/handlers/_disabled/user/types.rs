//! Shared types and utilities for user management

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::error::Result;

// ============================================================================
// Request/Response Types
// ============================================================================

/// User registration request with validation
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 50))]
    #[schema(example = "john_doe")]
    pub username: String,

    #[validate(email)]
    #[schema(example = "john.doe@example.com")]
    pub email: String,

    #[validate(length(min = 8, max = 128))]
    #[schema(example = "SecurePassword123!")]
    pub password: String,

    #[validate(length(min = 1, max = 100))]
    #[schema(example = "John")]
    pub first_name: String,

    #[validate(length(min = 1, max = 100))]
    #[schema(example = "Doe")]
    pub last_name: String,
}

/// Wallet address management request
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateWalletRequest {
    #[validate(length(min = 32, max = 44))]
    #[schema(example = "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8")]
    pub wallet_address: String,
    pub verify_ownership: Option<bool>,
}

/// Admin user update request
#[derive(Debug, Deserialize, Validate, Serialize, ToSchema)]
pub struct AdminUpdateUserRequest {
    #[validate(email)]
    pub email: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,

    #[validate(length(min = 1, max = 20))]
    pub role: Option<String>,

    pub is_active: Option<bool>,

    #[validate(length(min = 32, max = 44))]
    pub wallet_address: Option<String>,

    pub blockchain_registered: Option<bool>,
}

/// User activity log entry
#[derive(Debug, Serialize, ToSchema)]
pub struct UserActivity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub action: String,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// User activity response
#[derive(Debug, Serialize, ToSchema)]
pub struct UserActivityResponse {
    pub activities: Vec<UserActivity>,
    pub total: u64,
}

/// Registration response (without JWT - pending email verification)
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterResponse {
    pub message: String,
    pub email_verification_sent: bool,
}

/// Activity query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct ActivityQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Activity list response with pagination
#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityListResponse {
    pub activities: Vec<UserActivity>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

/// Database row for activity queries
#[derive(sqlx::FromRow)]
pub struct ActivityRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub activity_type: String,
    pub description: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Log user activity to database
pub async fn log_user_activity(
    db: &sqlx::PgPool,
    user_id: Uuid,
    action: String,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> Result<()> {
    let activity_id = Uuid::new_v4();

    // Use correct column names from migration
    let _ = sqlx::query(
        "INSERT INTO user_activities (id, user_id, activity_type, description, ip_address, user_agent, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())"
    )
    .bind(activity_id)
    .bind(user_id)
    .bind(action)  // action maps to activity_type
    .bind(details)  // details maps to description
    .bind(ip_address)
    .bind(user_agent)
    .execute(db)
    .await;

    Ok(())
}

/// Validate Solana wallet address format
pub fn is_valid_solana_address(address: &str) -> bool {
    // Basic Solana address validation (base58, 32-44 characters)
    if address.len() < 32 || address.len() > 44 {
        return false;
    }

    // Check if it's valid base58
    bs58::decode(address).into_vec().is_ok()
}
