//! Shared query parameter types for handler endpoints.
//!
//! This module provides standardized query types that can be used across
//! multiple handlers to ensure consistency and reduce code duplication.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::constants::pagination;
use crate::error::{ApiError, Result};
use crate::handlers::extractors::SortOrder;

/// Standard pagination query parameters used across all list endpoints.
///
/// # Example
/// ```ignore
/// async fn list_items(Query(query): Query<StandardPaginationQuery>) -> Result<Json<...>> {
///     query.validate()?;
///     // Use query.limit(), query.offset()
/// }
/// ```
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct StandardPaginationQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[validate(range(min = 1, max = 10000))]
    pub page: u32,

    /// Number of items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[validate(range(min = 1, max = 100))]
    pub per_page: u32,

    /// Sort direction: "asc" or "desc" (default: desc)
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_page() -> u32 {
    pagination::DEFAULT_PAGE
}

fn default_per_page() -> u32 {
    pagination::DEFAULT_PER_PAGE
}

impl Default for StandardPaginationQuery {
    fn default() -> Self {
        Self {
            page: pagination::DEFAULT_PAGE,
            per_page: pagination::DEFAULT_PER_PAGE,
            sort_order: SortOrder::Desc,
        }
    }
}

impl StandardPaginationQuery {
    /// Validate and normalize the pagination parameters
    pub fn validate_and_normalize(&mut self) -> Result<()> {
        // Ensure page is at least 1
        if self.page < 1 {
            self.page = 1;
        }

        // Clamp per_page to valid range
        self.per_page = self.per_page.clamp(
            pagination::MIN_PER_PAGE,
            pagination::MAX_PER_PAGE,
        );

        Ok(())
    }

    /// Get SQL LIMIT value
    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    /// Get SQL OFFSET value
    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    /// Get sort direction as SQL string
    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

/// User search query with pagination and filtering.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct UserSearchQuery {
    /// Search term for username, email, first name, or last name
    pub search: Option<String>,

    /// Filter by user role
    pub role: Option<String>,

    /// Filter by active status
    pub is_active: Option<bool>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field: "created_at", "username", "email", "role"
    #[serde(default = "default_user_sort_field")]
    pub sort_by: String,

    /// Sort direction: "asc" or "desc"
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_user_sort_field() -> String {
    "created_at".to_string()
}

impl UserSearchQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] = &["created_at", "username", "email", "role"];

    /// Validate and normalize query parameters
    pub fn validate_and_normalize(&mut self) -> Result<()> {
        // Normalize pagination
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self.per_page.clamp(1, 100);

        // Validate sort field
        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        // Trim and validate search term
        if let Some(ref mut search) = self.search {
            *search = search.trim().to_string();
            if search.is_empty() {
                self.search = None;
            }
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

/// Trading order query with filtering and pagination.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct OrderQuery {
    /// Filter by order status
    pub status: Option<String>,

    /// Filter by order side (buy/sell)
    pub side: Option<String>,

    /// Filter by order type (limit/market)
    pub order_type: Option<String>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field: "created_at", "price_per_kwh", "energy_amount", "filled_at"
    #[serde(default = "default_order_sort_field")]
    pub sort_by: String,

    /// Sort direction
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_order_sort_field() -> String {
    "created_at".to_string()
}

impl OrderQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] = 
        &["created_at", "price_per_kwh", "energy_amount", "filled_at", "updated_at"];

    pub fn validate_and_normalize(&mut self) -> Result<()> {
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self.per_page.clamp(1, 100);

        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

/// Meter readings query with filtering and pagination.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct MeterReadingsQuery {
    /// Filter by minted status
    pub minted: Option<bool>,

    /// Filter by meter ID
    pub meter_id: Option<uuid::Uuid>,

    /// Start date for filtering
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,

    /// End date for filtering
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field: "submitted_at", "reading_timestamp", "kwh_amount"
    #[serde(default = "default_reading_sort_field")]
    pub sort_by: String,

    /// Sort direction
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_reading_sort_field() -> String {
    "submitted_at".to_string()
}

impl MeterReadingsQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] = 
        &["submitted_at", "reading_timestamp", "kwh_amount", "created_at"];

    pub fn validate_and_normalize(&mut self) -> Result<()> {
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self.per_page.clamp(1, 100);

        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        // Validate date range
        if let (Some(start), Some(end)) = (&self.start_date, &self.end_date) {
            if start > end {
                return Err(ApiError::validation_error(
                    "start_date must be before end_date",
                    Some("date_range"),
                ));
            }
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

/// Transaction query with filtering and pagination.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct TransactionQuery {
    /// Filter by transaction status
    pub status: Option<String>,

    /// Filter by transaction type
    pub tx_type: Option<String>,

    /// Start date for filtering
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,

    /// End date for filtering  
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field
    #[serde(default = "default_tx_sort_field")]
    pub sort_by: String,

    /// Sort direction
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_tx_sort_field() -> String {
    "created_at".to_string()
}

impl TransactionQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] = 
        &["created_at", "amount", "status", "confirmed_at"];

    pub fn validate_and_normalize(&mut self) -> Result<()> {
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self.per_page.clamp(1, 100);

        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        if let (Some(start), Some(end)) = (&self.start_date, &self.end_date) {
            if start > end {
                return Err(ApiError::validation_error(
                    "start_date must be before end_date",
                    Some("date_range"),
                ));
            }
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

/// Audit log query with filtering and pagination.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema, IntoParams)]
pub struct AuditLogQuery {
    /// Filter by user ID
    pub user_id: Option<uuid::Uuid>,

    /// Filter by action type
    pub action: Option<String>,

    /// Filter by resource type
    pub resource_type: Option<String>,

    /// Start date for filtering
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,

    /// End date for filtering
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort direction (audit logs typically sorted by timestamp desc)
    #[serde(default)]
    pub sort_order: SortOrder,
}

impl AuditLogQuery {
    pub fn validate_and_normalize(&mut self) -> Result<()> {
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self.per_page.clamp(1, 100);

        if let (Some(start), Some(end)) = (&self.start_date, &self.end_date) {
            if start > end {
                return Err(ApiError::validation_error(
                    "start_date must be before end_date",
                    Some("date_range"),
                ));
            }
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_defaults() {
        let query = StandardPaginationQuery::default();
        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 20);
        assert_eq!(query.offset(), 0);
        assert_eq!(query.limit(), 20);
    }

    #[test]
    fn test_pagination_offset_calculation() {
        let mut query = StandardPaginationQuery {
            page: 3,
            per_page: 10,
            sort_order: SortOrder::Asc,
        };
        query.validate_and_normalize().unwrap();
        assert_eq!(query.offset(), 20);
        assert_eq!(query.limit(), 10);
    }

    #[test]
    fn test_user_search_validation() {
        let mut query = UserSearchQuery {
            search: Some("test".to_string()),
            role: None,
            is_active: None,
            page: 1,
            per_page: 20,
            sort_by: "username".to_string(),
            sort_order: SortOrder::Asc,
        };
        assert!(query.validate_and_normalize().is_ok());

        // Invalid sort field
        query.sort_by = "invalid".to_string();
        assert!(query.validate_and_normalize().is_err());
    }

    #[test]
    fn test_order_query_validation() {
        let mut query = OrderQuery {
            status: None,
            side: None,
            order_type: None,
            page: 0, // Invalid, should be normalized
            per_page: 200, // Over max, should be clamped
            sort_by: "created_at".to_string(),
            sort_order: SortOrder::Desc,
        };
        query.validate_and_normalize().unwrap();
        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 100);
    }
}
