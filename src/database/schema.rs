// Database schema definitions will be added here
// This module will contain SQL schema definitions and migrations

pub mod types {
    use serde::{Deserialize, Serialize};
    use utoipa::ToSchema;
    use std::fmt;

    #[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
    #[sqlx(type_name = "user_role", rename_all = "lowercase")]
    pub enum UserRole {
        User,
        Admin,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
    #[sqlx(type_name = "order_type")]
    pub enum OrderType {
        Market,
        Limit,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, ToSchema)]
    #[sqlx(type_name = "order_side")]
    pub enum OrderSide {
        Buy,
        Sell,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type, ToSchema)]
    #[sqlx(type_name = "order_status")]
    pub enum OrderStatus {
        Pending,
        Active,
        PartiallyFilled,
        Filled,
        Settled,
        Cancelled,
        Expired,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type, ToSchema)]
    #[sqlx(type_name = "epoch_status")]
    pub enum EpochStatus {
        Pending,
        Active,
        Cleared,
        Settled,
    }

    impl fmt::Display for EpochStatus {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                EpochStatus::Pending => write!(f, "pending"),
                EpochStatus::Active => write!(f, "active"),
                EpochStatus::Cleared => write!(f, "cleared"),
                EpochStatus::Settled => write!(f, "settled"),
            }
        }
    }
}
