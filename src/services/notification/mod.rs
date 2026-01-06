pub mod types;
pub mod email;

pub use types::*;
pub use email::EmailService;

use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, error};
use uuid::Uuid;
use chrono::Utc;

use crate::error::ApiError;

/// Notification service for sending emails and in-app notifications
#[derive(Clone, Debug)]
pub struct NotificationService {
    db: PgPool,
    email_service: EmailService,
}

impl NotificationService {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            email_service: EmailService::new(),
        }
    }

    /// Send a notification to a user
    pub async fn send_notification(
        &self,
        user_id: Uuid,
        notification_type: NotificationType,
        title: String,
        message: String,
        data: Option<serde_json::Value>,
    ) -> Result<Notification, ApiError> {
        let notification = Notification {
            id: Uuid::new_v4(),
            user_id,
            notification_type,
            title,
            message,
            data,
            created_at: Utc::now(),
            read_at: None,
        };

        // Store notification in database (optional - implement if you have a notifications table)
        // self.store_notification(&notification).await?;

        info!("📬 Notification created for user {}: {}", user_id, notification.title);

        Ok(notification)
    }

    /// Notify user of trade match
    pub async fn notify_trade_matched(
        &self,
        user_id: Uuid,
        user_email: &str,
        data: TradeMatchNotification,
    ) -> Result<(), ApiError> {
        // Send in-app notification
        let _ = self.send_notification(
            user_id,
            NotificationType::OrderMatched,
            "Order Matched".to_string(),
            format!("Your {} order for {} kWh has been matched at {} GRIDX/kWh",
                data.side, data.energy_amount, data.price_per_kwh),
            Some(serde_json::to_value(&data).unwrap_or_default()),
        ).await;

        // Send email notification
        if let Err(e) = self.email_service.send_email(
            user_email,
            "GridTokenX User",
            EmailTemplate::TradeMatched(data),
        ).await {
            error!("Failed to send trade match email: {}", e);
        }

        Ok(())
    }

    /// Notify user of settlement completion
    pub async fn notify_settlement_complete(
        &self,
        user_id: Uuid,
        user_email: &str,
        data: SettlementNotification,
    ) -> Result<(), ApiError> {
        let _ = self.send_notification(
            user_id,
            NotificationType::SettlementComplete,
            "Settlement Complete".to_string(),
            format!("Your settlement of {} kWh ({} GRIDX) has been completed",
                data.energy_amount, data.total_value),
            Some(serde_json::to_value(&data).unwrap_or_default()),
        ).await;

        if let Err(e) = self.email_service.send_email(
            user_email,
            "GridTokenX User",
            EmailTemplate::SettlementComplete(data),
        ).await {
            error!("Failed to send settlement email: {}", e);
        }

        Ok(())
    }

    /// Notify user of REC issuance
    pub async fn notify_rec_issued(
        &self,
        user_id: Uuid,
        user_email: &str,
        data: RecIssuedNotification,
    ) -> Result<(), ApiError> {
        let _ = self.send_notification(
            user_id,
            NotificationType::RecIssued,
            "REC Certificate Issued".to_string(),
            format!("A REC certificate ({}) for {} kWh has been issued",
                data.certificate_id, data.kwh_amount),
            Some(serde_json::to_value(&data).unwrap_or_default()),
        ).await;

        if let Err(e) = self.email_service.send_email(
            user_email,
            "GridTokenX User",
            EmailTemplate::RecIssued(data),
        ).await {
            error!("Failed to send REC email: {}", e);
        }

        Ok(())
    }

    /// Get user email from database
    pub async fn get_user_email(&self, user_id: &Uuid) -> Result<String, ApiError> {
        let result = sqlx::query_scalar!(
            "SELECT email FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::Database)?;

        result.ok_or_else(|| ApiError::NotFound("User not found".into()))
    }
}
