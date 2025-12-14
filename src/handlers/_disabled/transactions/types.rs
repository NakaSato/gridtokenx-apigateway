use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::models::transaction::TransactionFilters;

/// Pagination defaults for transaction queries
#[allow(dead_code)]
pub mod pagination {
    pub const DEFAULT_LIMIT: i64 = 20;
    pub const MAX_LIMIT: i64 = 100;
    pub const MIN_LIMIT: i64 = 1;
}

/// Query parameters for transaction endpoints
#[derive(Debug, Deserialize)]
pub struct TransactionQueryParams {
    pub operation_type: Option<String>,
    pub tx_type: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_attempts: Option<i32>,
    pub has_signature: Option<bool>,
}

impl TransactionQueryParams {
    pub fn into_transaction_filters(self, user_id: Option<Uuid>) -> TransactionFilters {
        TransactionFilters {
            operation_type: self.operation_type.and_then(|t| t.parse().ok()),
            tx_type: self.tx_type.and_then(|t| t.parse().ok()),
            status: self.status.and_then(|s| s.parse().ok()),
            user_id,
            date_from: self
                .date_from
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            date_to: self
                .date_to
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            limit: self.limit,
            offset: self.offset,
            min_attempts: self.min_attempts,
            has_signature: self.has_signature,
        }
    }
}
