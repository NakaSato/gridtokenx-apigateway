//! Service trait abstractions for dependency injection.
//!
//! This module defines traits for core services, enabling:
//! - Easier testing through mocking
//! - Loose coupling between components
//! - Flexible service implementations

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::ApiError;

/// Blockchain service trait for energy token operations
#[async_trait]
pub trait BlockchainServiceTrait: Send + Sync {
    /// Mint energy tokens for a user
    async fn mint_energy_tokens(
        &self,
        user_wallet: &str,
        amount: u64,
        energy_type: &str,
    ) -> Result<String, ApiError>;

    /// Transfer tokens between wallets
    async fn transfer_tokens(
        &self,
        from_wallet: &str,
        to_wallet: &str,
        amount: u64,
    ) -> Result<String, ApiError>;

    /// Get token balance for a wallet
    async fn get_balance(&self, wallet: &str) -> Result<u64, ApiError>;

    /// Verify a blockchain transaction
    async fn verify_transaction(&self, signature: &str) -> Result<bool, ApiError>;
}

/// User management service trait
#[async_trait]
pub trait UserServiceTrait: Send + Sync {
    /// Create a new user
    async fn create_user(&self, email: &str, password: &str) -> Result<Uuid, ApiError>;

    /// Find user by email
    async fn find_by_email(&self, email: &str) -> Result<Option<UserInfo>, ApiError>;

    /// Find user by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<UserInfo>, ApiError>;

    /// Update user profile
    async fn update_profile(&self, id: Uuid, profile: UpdateProfile) -> Result<(), ApiError>;

    /// Verify user email
    async fn verify_email(&self, token: &str) -> Result<(), ApiError>;
}

/// User information returned by service
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub wallet_address: Option<String>,
    pub is_verified: bool,
    pub role: String,
}

/// Profile update data
#[derive(Debug, Clone)]
pub struct UpdateProfile {
    pub display_name: Option<String>,
    pub wallet_address: Option<String>,
}

/// Energy meter service trait
#[async_trait]
pub trait MeterServiceTrait: Send + Sync {
    /// Register a new meter
    async fn register_meter(
        &self,
        user_id: Uuid,
        serial_number: &str,
        meter_type: &str,
    ) -> Result<Uuid, ApiError>;

    /// Submit meter reading
    async fn submit_reading(
        &self,
        meter_id: Uuid,
        reading: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<Uuid, ApiError>;

    /// Get meter readings for a time range
    async fn get_readings(
        &self,
        meter_id: Uuid,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MeterReading>, ApiError>;

    /// Verify meter ownership
    async fn verify_ownership(&self, meter_id: Uuid, user_id: Uuid) -> Result<bool, ApiError>;
}

/// Meter reading data
#[derive(Debug, Clone)]
pub struct MeterReading {
    pub id: Uuid,
    pub meter_id: Uuid,
    pub value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub verified: bool,
}

/// Trading service trait
#[async_trait]
pub trait TradingServiceTrait: Send + Sync {
    /// Place a buy order
    async fn place_buy_order(
        &self,
        user_id: Uuid,
        amount: u64,
        price: f64,
    ) -> Result<Uuid, ApiError>;

    /// Place a sell order
    async fn place_sell_order(
        &self,
        user_id: Uuid,
        amount: u64,
        price: f64,
    ) -> Result<Uuid, ApiError>;

    /// Cancel an order
    async fn cancel_order(&self, order_id: Uuid, user_id: Uuid) -> Result<(), ApiError>;

    /// Get order status
    async fn get_order(&self, order_id: Uuid) -> Result<Option<OrderInfo>, ApiError>;

    /// Get user's active orders
    async fn get_user_orders(&self, user_id: Uuid) -> Result<Vec<OrderInfo>, ApiError>;
}

/// Order information
#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub id: Uuid,
    pub user_id: Uuid,
    pub order_type: OrderType,
    pub amount: u64,
    pub price: f64,
    pub status: OrderStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
}

/// Cache service trait for generic caching operations
#[async_trait]
pub trait CacheServiceTrait: Send + Sync {
    /// Get a cached value
    async fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, ApiError>;

    /// Set a cached value with TTL
    async fn set<T: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> Result<(), ApiError>;

    /// Delete a cached value
    async fn delete(&self, key: &str) -> Result<(), ApiError>;

    /// Check if a key exists
    async fn exists(&self, key: &str) -> Result<bool, ApiError>;

    /// Increment a counter
    async fn increment(&self, key: &str) -> Result<i64, ApiError>;
}

/// Notification service trait
#[async_trait]
pub trait NotificationServiceTrait: Send + Sync {
    /// Send an email notification
    async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), ApiError>;

    /// Send a verification email
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), ApiError>;

    /// Send a password reset email
    async fn send_password_reset(&self, to: &str, token: &str) -> Result<(), ApiError>;
}

/// Audit logging service trait
#[async_trait]
pub trait AuditServiceTrait: Send + Sync {
    /// Log an audit event
    async fn log_event(&self, event: AuditEvent) -> Result<(), ApiError>;

    /// Get audit logs for a user
    async fn get_user_logs(
        &self,
        user_id: Uuid,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<AuditEvent>, ApiError>;
}

/// Audit event data
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AuditEvent {
    pub fn new(action: impl Into<String>, resource_type: impl Into<String>) -> Self {
        Self {
            id: None,
            user_id: None,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: None,
            details: serde_json::Value::Null,
            ip_address: None,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_resource(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = Some(resource_id.into());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }
}
