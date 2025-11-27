// Audit logging service for security events tracking
// Logs authentication, authorization, trading, and security events to database

use sqlx::PgPool;
use serde::{Serialize, Deserialize};
use chrono::Utc;
use uuid::Uuid;
use utoipa::ToSchema;
use sqlx::types::ipnetwork::IpNetwork;

/// Security and business events to be audited
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// User successfully logged in
    UserLogin { 
        user_id: Uuid, 
        ip: String,
        user_agent: Option<String>,
    },
    /// User logged out
    UserLogout { 
        user_id: Uuid 
    },
    /// Login attempt failed
    LoginFailed { 
        email: String, 
        ip: String, 
        reason: String,
        user_agent: Option<String>,
    },
    /// User password was changed
    PasswordChanged { 
        user_id: Uuid, 
        ip: String 
    },
    /// Email verification completed
    EmailVerified { 
        user_id: Uuid 
    },
    /// New API key generated
    ApiKeyGenerated { 
        user_id: Uuid, 
        key_id: Uuid 
    },
    /// User registered on blockchain
    BlockchainRegistration { 
        user_id: Uuid, 
        wallet_address: String 
    },
    /// Trading order created
    OrderCreated { 
        user_id: Uuid, 
        order_id: Uuid, 
        order_type: String,
        amount: String,
        price: String,
    },
    /// Trading order cancelled
    OrderCancelled { 
        user_id: Uuid, 
        order_id: Uuid 
    },
    /// Trading order matched
    OrderMatched {
        buyer_id: Uuid,
        seller_id: Uuid,
        order_id: Uuid,
        amount: String,
    },
    /// Unauthorized access attempt
    UnauthorizedAccess { 
        ip: String, 
        endpoint: String,
        user_agent: Option<String>,
    },
    /// Rate limit exceeded
    RateLimitExceeded { 
        ip: String, 
        endpoint: String 
    },
    /// Sensitive data accessed
    DataAccess {
        user_id: Uuid,
        resource_type: String,
        resource_id: String,
        action: String,
    },
    /// Admin action performed
    AdminAction {
        admin_id: Uuid,
        action: String,
        target_user_id: Option<Uuid>,
        details: String,
    },
}

impl AuditEvent {
    /// Get the event type as a string for database storage
    pub fn event_type(&self) -> &'static str {
        match self {
            AuditEvent::UserLogin { .. } => "user_login",
            AuditEvent::UserLogout { .. } => "user_logout",
            AuditEvent::LoginFailed { .. } => "login_failed",
            AuditEvent::PasswordChanged { .. } => "password_changed",
            AuditEvent::EmailVerified { .. } => "email_verified",
            AuditEvent::ApiKeyGenerated { .. } => "api_key_generated",
            AuditEvent::BlockchainRegistration { .. } => "blockchain_registration",
            AuditEvent::OrderCreated { .. } => "order_created",
            AuditEvent::OrderCancelled { .. } => "order_cancelled",
            AuditEvent::OrderMatched { .. } => "order_matched",
            AuditEvent::UnauthorizedAccess { .. } => "unauthorized_access",
            AuditEvent::RateLimitExceeded { .. } => "rate_limit_exceeded",
            AuditEvent::DataAccess { .. } => "data_access",
            AuditEvent::AdminAction { .. } => "admin_action",
        }
    }

    /// Extract user_id if present in the event
    pub fn user_id(&self) -> Option<Uuid> {
        match self {
            AuditEvent::UserLogin { user_id, .. }
            | AuditEvent::UserLogout { user_id }
            | AuditEvent::PasswordChanged { user_id, .. }
            | AuditEvent::EmailVerified { user_id }
            | AuditEvent::ApiKeyGenerated { user_id, .. }
            | AuditEvent::BlockchainRegistration { user_id, .. }
            | AuditEvent::OrderCreated { user_id, .. }
            | AuditEvent::OrderCancelled { user_id, .. }
            | AuditEvent::DataAccess { user_id, .. } 
            | AuditEvent::AdminAction { admin_id: user_id, .. } => Some(*user_id),
            AuditEvent::OrderMatched { buyer_id, .. } => Some(*buyer_id), // Prioritize buyer for indexing
            _ => None,
        }
    }

    /// Extract IP address if present in the event
    pub fn ip_address(&self) -> Option<&str> {
        match self {
            AuditEvent::UserLogin { ip, .. }
            | AuditEvent::LoginFailed { ip, .. }
            | AuditEvent::PasswordChanged { ip, .. }
            | AuditEvent::UnauthorizedAccess { ip, .. }
            | AuditEvent::RateLimitExceeded { ip, .. } => Some(ip.as_str()),
            _ => None,
        }
    }
}

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
        let ip_address = ip_address_str.as_deref().and_then(|s| s.parse::<IpNetwork>().ok());
        let event_data = serde_json::to_value(&event)
            .expect("Failed to serialize audit event");
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

/// Audit event database record
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditEventRecord {
    pub id: Uuid,
    pub event_type: String,
    pub user_id: Option<Uuid>,
    #[schema(value_type = Option<String>)]
    pub ip_address: Option<IpNetwork>,
    pub event_data: serde_json::Value,
    pub created_at: Option<chrono::DateTime<Utc>>,
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
