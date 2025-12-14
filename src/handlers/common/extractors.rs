//! Common extractors for API handlers.
//!
//! This module provides reusable types and utilities for request validation
//! and parameter extraction that can be used across handlers.

use uuid::Uuid;

use crate::error::ApiError;

/// Validated UUID helper
/// 
/// Use this to parse and validate UUIDs from string parameters.
///
/// # Example
/// ```ignore
/// let uuid = ValidatedUuid::parse(&id_string)?;
/// ```
pub struct ValidatedUuid;

impl ValidatedUuid {
    /// Parse a string into a UUID, returning an ApiError on failure
    pub fn parse(s: &str) -> Result<Uuid, ApiError> {
        Uuid::parse_str(s)
            .map_err(|_| ApiError::validation_error(format!("Invalid UUID: {}", s), Some("id")))
    }
}

/// Pagination parameters with defaults
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,

    #[serde(default = "default_per_page")]
    pub per_page: u32,

    #[serde(default)]
    pub sort_by: Option<String>,

    #[serde(default)]
    pub sort_order: Option<SortOrder>,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
            sort_by: None,
            sort_order: None,
        }
    }
}

impl PaginationParams {
    /// Create new pagination params
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, 100),
            sort_by: None,
            sort_order: None,
        }
    }

    /// Set sort parameters
    pub fn with_sort(mut self, sort_by: impl Into<String>, order: SortOrder) -> Self {
        self.sort_by = Some(sort_by.into());
        self.sort_order = Some(order);
        self
    }

    /// Calculate the offset for SQL queries
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }

    /// Get the limit for SQL queries
    pub fn limit(&self) -> u32 {
        self.per_page.clamp(1, 100)
    }

    /// Validate pagination parameters
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.page == 0 {
            return Err(ApiError::validation_error("Page must be at least 1", Some("page")));
        }
        if self.per_page == 0 || self.per_page > 100 {
            return Err(ApiError::validation_error("Per page must be between 1 and 100", Some("per_page")));
        }
        Ok(())
    }
}

/// Sort order for queries
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, Default, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl SortOrder {
    /// Get the SQL keyword for this sort order
    pub fn as_sql(&self) -> &'static str {
        match self {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }

    /// Check if ascending
    pub fn is_asc(&self) -> bool {
        matches!(self, SortOrder::Asc)
    }

    /// Check if descending
    pub fn is_desc(&self) -> bool {
        matches!(self, SortOrder::Desc)
    }
}

/// Date range filter parameters
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct DateRangeParams {
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
}

impl DateRangeParams {
    /// Create a new date range
    pub fn new(
        start: Option<chrono::DateTime<chrono::Utc>>,
        end: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        Self {
            start_date: start,
            end_date: end,
        }
    }

    /// Validate that start_date is before end_date
    pub fn validate(&self) -> Result<(), ApiError> {
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

    /// Check if both dates are set
    pub fn is_complete(&self) -> bool {
        self.start_date.is_some() && self.end_date.is_some()
    }

    /// Check if range is empty (no dates set)
    pub fn is_empty(&self) -> bool {
        self.start_date.is_none() && self.end_date.is_none()
    }
}

/// Search/filter parameters
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct SearchParams {
    pub query: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
}

impl SearchParams {
    /// Check if any search criteria is set
    pub fn has_criteria(&self) -> bool {
        self.query.is_some() || self.status.is_some() || self.tags.as_ref().map(|t| !t.is_empty()).unwrap_or(false)
    }

    /// Get the search query, trimmed
    pub fn query_trimmed(&self) -> Option<&str> {
        self.query.as_ref().map(|q| q.trim()).filter(|q| !q.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_defaults() {
        let params = PaginationParams::default();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 20);
        assert_eq!(params.offset(), 0);
        assert_eq!(params.limit(), 20);
    }

    #[test]
    fn test_pagination_offset() {
        let params = PaginationParams::new(3, 10);
        assert_eq!(params.offset(), 20);
        assert_eq!(params.limit(), 10);
    }

    #[test]
    fn test_pagination_clamp() {
        let params = PaginationParams::new(0, 200);
        assert_eq!(params.page, 1);
        assert_eq!(params.limit(), 100); // clamped
    }

    #[test]
    fn test_sort_order_sql() {
        assert_eq!(SortOrder::Asc.as_sql(), "ASC");
        assert_eq!(SortOrder::Desc.as_sql(), "DESC");
    }

    #[test]
    fn test_validated_uuid() {
        assert!(ValidatedUuid::parse("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(ValidatedUuid::parse("invalid").is_err());
    }

    #[test]
    fn test_date_range_validation() {
        use chrono::TimeZone;
        
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
        
        // Valid range
        let valid = DateRangeParams::new(Some(start), Some(end));
        assert!(valid.validate().is_ok());
        
        // Invalid range (start > end)
        let invalid = DateRangeParams::new(Some(end), Some(start));
        assert!(invalid.validate().is_err());
    }
}
