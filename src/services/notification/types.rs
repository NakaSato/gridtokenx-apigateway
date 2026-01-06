use serde::{Deserialize, Serialize};
use uuid::Uuid;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

/// Types of notifications the system can send
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// Order has been matched with another order
    OrderMatched,
    /// Settlement has been completed
    SettlementComplete,
    /// REC has been issued
    RecIssued,
    /// Order has been fully filled
    OrderFilled,
    /// Order has been partially filled
    OrderPartiallyFilled,
    /// Order has been cancelled
    OrderCancelled,
    /// System alert
    SystemAlert,
}

/// Notification payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// Trade match notification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMatchNotification {
    pub order_id: Uuid,
    pub match_id: Uuid,
    pub counterparty_id: Uuid,
    pub energy_amount: Decimal,
    pub price_per_kwh: Decimal,
    pub total_value: Decimal,
    pub side: String, // "buy" or "sell"
}

/// Settlement complete notification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementNotification {
    pub settlement_id: Uuid,
    pub energy_amount: Decimal,
    pub total_value: Decimal,
    pub tx_signature: Option<String>,
}

/// REC issued notification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecIssuedNotification {
    pub certificate_id: String,
    pub settlement_id: Uuid,
    pub kwh_amount: Decimal,
    pub renewable_source: String,
}

/// Email template type
#[derive(Debug, Clone)]
pub enum EmailTemplate {
    TradeMatched(TradeMatchNotification),
    SettlementComplete(SettlementNotification),
    RecIssued(RecIssuedNotification),
}

impl EmailTemplate {
    pub fn subject(&self) -> String {
        match self {
            EmailTemplate::TradeMatched(_) => "🤝 Your Order Has Been Matched".to_string(),
            EmailTemplate::SettlementComplete(_) => "✅ Settlement Complete".to_string(),
            EmailTemplate::RecIssued(_) => "🏆 REC Issued".to_string(),
        }
    }
}
