use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use utoipa::ToSchema;
use crate::database::schema::types::UserRole;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub department: String,
    pub wallet_address: Option<String>,
    pub blockchain_registered: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub department: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserProfile {
    pub user: User,
    pub balances: UserBalances,
    pub meter_assignments: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserBalances {
    #[schema(value_type = f64)]
    pub grid_tokens: rust_decimal::Decimal,
    #[schema(value_type = f64)]
    pub pending_trades: rust_decimal::Decimal,
}