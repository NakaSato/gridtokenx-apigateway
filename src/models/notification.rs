//! Notification Models
//!
//! Data structures for push notifications system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// Type of notification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "notification_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// Order was filled (fully or partially)
    OrderFilled,
    /// Order was matched with counterparty
    OrderMatched,
    /// Conditional order (stop-loss/take-profit) was triggered
    ConditionalTriggered,
    /// Recurring order was executed
    RecurringExecuted,
    /// Price alert threshold reached
    PriceAlert,
    /// Escrow funds released
    EscrowReleased,
    /// System announcement
    System,
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationType::OrderFilled => write!(f, "order_filled"),
            NotificationType::OrderMatched => write!(f, "order_matched"),
            NotificationType::ConditionalTriggered => write!(f, "conditional_triggered"),
            NotificationType::RecurringExecuted => write!(f, "recurring_executed"),
            NotificationType::PriceAlert => write!(f, "price_alert"),
            NotificationType::EscrowReleased => write!(f, "escrow_released"),
            NotificationType::System => write!(f, "system"),
        }
    }
}

/// A notification record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

/// User notification preferences
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct NotificationPreferences {
    pub user_id: Uuid,
    pub order_filled: bool,
    pub order_matched: bool,
    pub conditional_triggered: bool,
    pub recurring_executed: bool,
    pub price_alerts: bool,
    pub escrow_events: bool,
    pub system_announcements: bool,
    pub email_enabled: bool,
    pub push_enabled: bool,
    pub updated_at: DateTime<Utc>,
}

/// Request to update notification preferences
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdatePreferencesRequest {
    pub order_filled: Option<bool>,
    pub order_matched: Option<bool>,
    pub conditional_triggered: Option<bool>,
    pub recurring_executed: Option<bool>,
    pub price_alerts: Option<bool>,
    pub escrow_events: Option<bool>,
    pub system_announcements: Option<bool>,
    pub email_enabled: Option<bool>,
    pub push_enabled: Option<bool>,
}

/// WebSocket notification message
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebSocketNotification {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: Notification,
}

impl WebSocketNotification {
    pub fn new(notification: Notification) -> Self {
        Self {
            msg_type: "notification".to_string(),
            data: notification,
        }
    }
}

/// Request to create a notification (internal use)
#[derive(Debug, Clone)]
pub struct CreateNotificationRequest {
    pub user_id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}

/// Response for listing notifications
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationListResponse {
    pub notifications: Vec<Notification>,
    pub unread_count: i64,
    pub total: i64,
}
