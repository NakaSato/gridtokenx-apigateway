//! Common response types and helpers for API handlers.
//!
//! This module provides standardized response types that ensure consistency
//! across all API endpoints.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Standard API response wrapper
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Response metadata for pagination and additional info
#[derive(Debug, Serialize, ToSchema)]
pub struct ResponseMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Create a successful response with data
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            meta: None,
        }
    }

    /// Create a successful response with message
    pub fn success_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
            meta: None,
        }
    }

    /// Add metadata to response
    pub fn with_meta(mut self, meta: ResponseMeta) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Add pagination metadata
    pub fn with_pagination(mut self, page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        self.meta = Some(ResponseMeta {
            page: Some(page),
            per_page: Some(per_page),
            total: Some(total),
            total_pages: Some(total_pages),
            request_id: None,
        });
        self
    }
}

impl ApiResponse<()> {
    /// Create a successful response without data
    pub fn ok() -> Self {
        Self {
            success: true,
            data: None,
            message: None,
            meta: None,
        }
    }

    /// Create a successful response with only a message
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: None,
            message: Some(message.into()),
            meta: None,
        }
    }
}

/// Response helper for created resources
pub struct Created<T>(pub T);

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        (StatusCode::CREATED, Json(ApiResponse::success(self.0))).into_response()
    }
}

/// Response helper for no content
pub struct NoContent;

impl IntoResponse for NoContent {
    fn into_response(self) -> Response {
        StatusCode::NO_CONTENT.into_response()
    }
}

/// Response helper for accepted (async processing)
pub struct Accepted<T>(pub T);

impl<T: Serialize> IntoResponse for Accepted<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::ACCEPTED,
            Json(ApiResponse::success_with_message(
                self.0,
                "Request accepted for processing",
            )),
        )
            .into_response()
    }
}

/// Paginated response wrapper
#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginationInfo {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T: Serialize> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        Self {
            items,
            pagination: PaginationInfo {
                page,
                per_page,
                total,
                total_pages,
                has_next: page < total_pages,
                has_prev: page > 1,
            },
        }
    }
}

/// Simple list response without pagination
#[derive(Debug, Serialize, ToSchema)]
pub struct ListResponse<T: Serialize> {
    pub items: Vec<T>,
    pub count: usize,
}

impl<T: Serialize> ListResponse<T> {
    pub fn new(items: Vec<T>) -> Self {
        let count = items.len();
        Self { items, count }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data");
        assert!(response.success);
        assert_eq!(response.data, Some("test data"));
    }

    #[test]
    fn test_api_response_pagination() {
        let response = ApiResponse::success(vec![1, 2, 3]).with_pagination(1, 10, 100);
        let meta = response.meta.unwrap();
        assert_eq!(meta.total_pages, Some(10));
    }

    #[test]
    fn test_paginated_response() {
        let response = PaginatedResponse::new(vec![1, 2, 3], 1, 10, 25);
        assert_eq!(response.pagination.total_pages, 3);
        assert!(response.pagination.has_next);
        assert!(!response.pagination.has_prev);
    }
}
