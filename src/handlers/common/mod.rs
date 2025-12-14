//! Common utilities for API handlers.
//!
//! This module provides shared extractors, response types, and utilities
//! used across all handler modules.

pub mod extractors;
pub mod response;

// Re-export commonly used types
pub use extractors::{DateRangeParams, PaginationParams, SearchParams, SortOrder, ValidatedUuid};
pub use response::{ApiResponse, ListResponse, PaginatedResponse};
