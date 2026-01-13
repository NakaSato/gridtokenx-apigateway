//! Notification Dispatcher Service
//!
//! Handles creating, storing, and broadcasting notifications via WebSocket

use sqlx::PgPool;

use tokio::sync::broadcast;
use tracing::{info, error, warn};
use uuid::Uuid;

use crate::models::notification::{
    Notification, NotificationType, CreateNotificationRequest,
};

/// Message sent via broadcast channel
#[derive(Debug, Clone)]
pub struct BroadcastNotification {
    pub user_id: Uuid,
    pub notification: Notification,
}

/// Notification dispatcher configuration
#[derive(Debug, Clone)]
pub struct NotificationDispatcherConfig {
    /// Whether to check user preferences before sending
    pub respect_preferences: bool,
    /// Channel capacity for broadcast
    pub channel_capacity: usize,
}

impl Default for NotificationDispatcherConfig {
    fn default() -> Self {
        Self {
            respect_preferences: true,
            channel_capacity: 1000,
        }
    }
}

/// Notification dispatcher service
#[derive(Clone)]
pub struct NotificationDispatcher {
    db: PgPool,
    config: NotificationDispatcherConfig,
    broadcast_tx: broadcast::Sender<BroadcastNotification>,
    email_service: Option<crate::services::EmailService>,
}

impl NotificationDispatcher {
    pub fn new(
        db: PgPool,
        config: NotificationDispatcherConfig,
        email_service: Option<crate::services::EmailService>,
    ) -> Self {
        let (tx, _) = broadcast::channel(config.channel_capacity);
        Self {
            db,
            config,
            broadcast_tx: tx,
            email_service,
        }
    }

    /// Get a receiver for broadcast notifications
    pub fn subscribe(&self) -> broadcast::Receiver<BroadcastNotification> {
        self.broadcast_tx.subscribe()
    }

    /// Send a notification to a user
    pub async fn send(&self, request: CreateNotificationRequest) -> anyhow::Result<Notification> {
        // Check user preferences
        let (should_push, should_email) = self.check_preferences(request.user_id, &request.notification_type).await?;

        if self.config.respect_preferences && !should_push {
            info!("Notification suppressed by user preferences: {:?}", request.notification_type);
            // Still create the notification but don't broadcast
            return self.create_notification(request, false).await;
        }

        let notification = self.create_notification(request.clone(), true).await?;

        // Send email if enabled in preferences and email service is available
        if should_email {
            if let Some(email_service) = &self.email_service {
                // Get user email and username for the email
                let user_info = sqlx::query!(
                    "SELECT email, username FROM users WHERE id = $1",
                    request.user_id
                )
                .fetch_optional(&self.db)
                .await?;

                if let Some(user) = user_info {
                    let title = request.title.clone();
                    let message = request.message.clone().unwrap_or_default();
                    let email_service = email_service.clone();
                    let email = user.email.clone();
                    let username = user.username.clone();

                    // Send email asynchronously
                    tokio::spawn(async move {
                        if let Err(e) = email_service.send_notification_email(&email, &username, &title, &message).await {
                            error!("Failed to send notification email: {}", e);
                        }
                    });
                }
            }
        }

        Ok(notification)
    }

    /// Send notification to multiple users
    pub async fn send_bulk(&self, requests: Vec<CreateNotificationRequest>) -> anyhow::Result<Vec<Notification>> {
        let mut notifications = Vec::with_capacity(requests.len());
        
        for request in requests {
            match self.send(request).await {
                Ok(n) => notifications.push(n),
                Err(e) => error!("Failed to send notification: {}", e),
            }
        }

        Ok(notifications)
    }

    /// Send system-wide announcement
    pub async fn send_system_announcement(&self, title: &str, message: &str) -> anyhow::Result<i64> {
        // Get all users with system_announcements enabled
        let users = sqlx::query!(
            r#"
            SELECT u.id as "id!"
            FROM users u
            LEFT JOIN user_notification_preferences p ON u.id = p.user_id
            WHERE COALESCE(p.system_announcements, true) = true
            "#
        )
        .fetch_all(&self.db)
        .await?;

        let mut count = 0i64;
        for user in users {
            let request = CreateNotificationRequest {
                user_id: user.id,
                notification_type: NotificationType::System,
                title: title.to_string(),
                message: Some(message.to_string()),
                data: None,
            };

            if let Ok(_) = self.send(request).await {
                count += 1;
            }
        }

        info!("Sent system announcement to {} users", count);
        Ok(count)
    }

    /// Check if user wants to receive this notification type
    async fn check_preferences(&self, user_id: Uuid, notification_type: &NotificationType) -> anyhow::Result<(bool, bool)> {
        let prefs = sqlx::query!(
            r#"
            SELECT order_filled, order_matched, conditional_triggered,
                   recurring_executed, price_alerts, escrow_events, 
                   system_announcements, push_enabled, email_enabled
            FROM user_notification_preferences
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.db)
        .await?;

        // Default to (true, false) if no preferences set
        let Some(prefs) = prefs else {
            return Ok((true, false));
        };

        // Check specific notification type enabledness
        let type_enabled = match notification_type {
            NotificationType::OrderFilled => prefs.order_filled.unwrap_or(true),
            NotificationType::OrderMatched => prefs.order_matched.unwrap_or(true),
            NotificationType::ConditionalTriggered => prefs.conditional_triggered.unwrap_or(true),
            NotificationType::RecurringExecuted => prefs.recurring_executed.unwrap_or(true),
            NotificationType::PriceAlert => prefs.price_alerts.unwrap_or(true),
            NotificationType::EscrowReleased => prefs.escrow_events.unwrap_or(true),
            NotificationType::System => prefs.system_announcements.unwrap_or(true),
        };

        let push_enabled = type_enabled && prefs.push_enabled.unwrap_or(true);
        let email_enabled = type_enabled && prefs.email_enabled.unwrap_or(false);

        Ok((push_enabled, email_enabled))
    }

    /// Create notification in database and optionally broadcast
    async fn create_notification(&self, request: CreateNotificationRequest, broadcast: bool) -> anyhow::Result<Notification> {
        let notification = sqlx::query_as!(
            Notification,
            r#"
            INSERT INTO notifications (user_id, notification_type, title, message, data)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, user_id, notification_type as "notification_type!: NotificationType",
                      title, message, data, read as "read!", created_at as "created_at!"
            "#,
            request.user_id,
            request.notification_type as NotificationType,
            request.title,
            request.message,
            request.data
        )
        .fetch_one(&self.db)
        .await?;

        if broadcast {
            // Broadcast via channel (WebSocket handlers will pick this up)
            let broadcast_msg = BroadcastNotification {
                user_id: request.user_id,
                notification: notification.clone(),
            };

            if let Err(_) = self.broadcast_tx.send(broadcast_msg) {
                // No receivers - this is fine, just means no one is connected
                warn!("No WebSocket receivers for notification broadcast");
            }
        }

        info!("Created notification {} for user {}", notification.id, notification.user_id);
        Ok(notification)
    }
}

// Convenience functions for common notification types
impl NotificationDispatcher {
    pub async fn notify_order_filled(
        &self,
        user_id: Uuid,
        order_id: Uuid,
        amount: f64,
        price: f64,
    ) -> anyhow::Result<Notification> {
        self.send(CreateNotificationRequest {
            user_id,
            notification_type: NotificationType::OrderFilled,
            title: "Order Filled".to_string(),
            message: Some(format!("Your order for {:.2} kWh was filled at {:.4}/kWh", amount, price)),
            data: Some(serde_json::json!({
                "order_id": order_id,
                "amount": amount,
                "price": price
            })),
        }).await
    }

    pub async fn notify_conditional_triggered(
        &self,
        user_id: Uuid,
        order_id: Uuid,
        trigger_type: &str,
        trigger_price: f64,
    ) -> anyhow::Result<Notification> {
        self.send(CreateNotificationRequest {
            user_id,
            notification_type: NotificationType::ConditionalTriggered,
            title: format!("{} Triggered", trigger_type),
            message: Some(format!("Your {} order was triggered at price {:.4}", trigger_type, trigger_price)),
            data: Some(serde_json::json!({
                "order_id": order_id,
                "trigger_type": trigger_type,
                "trigger_price": trigger_price
            })),
        }).await
    }

    pub async fn notify_recurring_executed(
        &self,
        user_id: Uuid,
        recurring_id: Uuid,
        execution_number: i32,
        amount: f64,
    ) -> anyhow::Result<Notification> {
        self.send(CreateNotificationRequest {
            user_id,
            notification_type: NotificationType::RecurringExecuted,
            title: "Recurring Order Executed".to_string(),
            message: Some(format!("Execution #{} completed for {:.2} kWh", execution_number, amount)),
            data: Some(serde_json::json!({
                "recurring_id": recurring_id,
                "execution_number": execution_number,
                "amount": amount
            })),
        }).await
    }
}
