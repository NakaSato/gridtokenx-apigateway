use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::services::AuditEventRecord;

/// Query parameters for audit logs
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct AuditLogQuery {
    /// Filter by event type
    pub event_type: Option<String>,

    /// Filter by user ID
    pub user_id: Option<Uuid>,

    /// Filter by IP address
    pub ip_address: Option<String>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    50
}

/// Response for audit log queries
#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogsResponse {
    pub events: Vec<AuditEventRecord>,
    pub total: usize,
    pub page: u32,
    pub limit: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pagination() {
        let query = AuditLogQuery {
            event_type: None,
            user_id: None,
            ip_address: None,
            page: default_page(),
            limit: default_limit(),
        };

        assert_eq!(query.page, 1);
        assert_eq!(query.limit, 50);
    }
}
