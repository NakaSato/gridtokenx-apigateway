use chrono::Utc;
use sqlx::types::ipnetwork::IpNetwork;
use sqlx::PgPool;
use uuid::Uuid;

pub mod types;
pub use types::{AuditEvent, AuditEventRecord};

/// Audit logger service
#[derive(Debug, Clone)]
pub struct AuditLogger {
    db: PgPool,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Log an audit event to the database
    pub async fn log(&self, event: AuditEvent) -> Result<(), sqlx::Error> {
        let event_type = event.event_type();
        let user_id = event.user_id();
        let ip_address_str = event.ip_address().map(|s| s.to_string());
        let ip_address = ip_address_str
            .as_deref()
            .and_then(|s| s.parse::<IpNetwork>().ok());
        let event_data = serde_json::to_value(&event).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize audit event: {}", e);
            serde_json::json!({"error": "serialization_failed", "event_type": event_type})
        });
        let created_at = Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO audit_logs (event_type, user_id, ip_address, event_data, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            event_type,
            user_id,
            ip_address as _,
            event_data,
            created_at
        )
        .execute(&self.db)
        .await?;

        // Log to application logs as well for immediate visibility
        tracing::info!(
            event_type = event_type,
            user_id = ?user_id,
            ip = ?ip_address,
            "Audit event logged"
        );

        Ok(())
    }

    /// Log event without awaiting (fire-and-forget)
    /// Useful for non-critical logging that shouldn't block the request
    pub fn log_async(&self, event: AuditEvent) {
        let logger = self.clone();
        tokio::spawn(async move {
            if let Err(e) = logger.log(event).await {
                tracing::error!(error = %e, "Failed to log audit event");
            }
        });
    }

    /// Query recent events for a user
    pub async fn get_user_events(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditEventRecord>, sqlx::Error> {
        let records = sqlx::query_as!(
            AuditEventRecord,
            r#"
            SELECT id, event_type, user_id, ip_address, event_data, created_at
            FROM audit_logs
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&self.db)
        .await?;

        Ok(records)
    }

    /// Query events by type
    pub async fn get_events_by_type(
        &self,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<AuditEventRecord>, sqlx::Error> {
        let records = sqlx::query_as!(
            AuditEventRecord,
            r#"
            SELECT id, event_type, user_id, ip_address, event_data, created_at
            FROM audit_logs
            WHERE event_type = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            event_type,
            limit
        )
        .fetch_all(&self.db)
        .await?;

        Ok(records)
    }

    /// Get recent security events (unauthorized access, failed logins, rate limits)
    pub async fn get_security_events(
        &self,
        limit: i64,
    ) -> Result<Vec<AuditEventRecord>, sqlx::Error> {
        let records = sqlx::query_as!(
            AuditEventRecord,
            r#"
            SELECT id, event_type, user_id, ip_address, event_data, created_at
            FROM audit_logs
            WHERE event_type IN ('unauthorized_access', 'login_failed', 'rate_limit_exceeded')
            ORDER BY created_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(&self.db)
        .await?;

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_extraction() {
        let event = AuditEvent::UserLogin {
            user_id: Uuid::new_v4(),
            ip: "127.0.0.1".to_string(),
            user_agent: Some("Mozilla/5.0".to_string()),
        };
        assert_eq!(event.event_type(), "user_login");

        let event = AuditEvent::LoginFailed {
            email: "test@example.com".to_string(),
            ip: "127.0.0.1".to_string(),
            reason: "Invalid password".to_string(),
            user_agent: None,
        };
        assert_eq!(event.event_type(), "login_failed");
    }

    #[test]
    fn test_user_id_extraction() {
        let user_id = Uuid::new_v4();
        let event = AuditEvent::UserLogin {
            user_id,
            ip: "127.0.0.1".to_string(),
            user_agent: None,
        };
        assert_eq!(event.user_id(), Some(user_id));

        let event = AuditEvent::RateLimitExceeded {
            ip: "127.0.0.1".to_string(),
            endpoint: "/api/auth/login".to_string(),
        };
        assert_eq!(event.user_id(), None);
    }

    #[test]
    fn test_ip_extraction() {
        let event = AuditEvent::UserLogin {
            user_id: Uuid::new_v4(),
            ip: "192.168.1.100".to_string(),
            user_agent: None,
        };
        assert_eq!(event.ip_address(), Some("192.168.1.100"));

        let event = AuditEvent::EmailVerified {
            user_id: Uuid::new_v4(),
        };
        assert_eq!(event.ip_address(), None);
    }

    #[test]
    fn test_event_serialization() {
        let event = AuditEvent::OrderCreated {
            user_id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            order_type: "buy".to_string(),
            amount: "100.5".to_string(),
            price: "0.15".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "order_created");
        assert!(json["order_id"].is_string());
    }
}
