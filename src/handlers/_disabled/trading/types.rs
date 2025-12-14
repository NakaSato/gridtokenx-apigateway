use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::models::trading::TradingOrder;

/// Query parameters for trading orders
#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
pub struct OrderQuery {
    /// Filter by order status
    pub status: Option<OrderStatus>,

    /// Filter by order side (buy/sell)
    pub side: Option<OrderSide>,

    /// Filter by order type (limit/market)
    pub order_type: Option<OrderType>,

    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_page_size")]
    pub page_size: u32,

    /// Sort field: "created_at", "price_per_kwh", "energy_amount"
    pub sort_by: Option<String>,

    /// Sort direction: "asc" or "desc"
    #[serde(default = "default_sort_order")]
    pub sort_order: crate::utils::SortOrder,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

fn default_sort_order() -> crate::utils::SortOrder {
    crate::utils::SortOrder::Desc
}

impl OrderQuery {
    pub fn validate_params(&mut self) -> crate::error::Result<()> {
        if self.page < 1 {
            self.page = 1;
        }

        if self.page_size < 1 {
            self.page_size = 20;
        } else if self.page_size > 100 {
            self.page_size = 100;
        }

        // Validate sort field
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "created_at" | "price_per_kwh" | "energy_amount" | "filled_at" => {}
                _ => {
                    return Err(crate::error::ApiError::validation_error(
                        "Invalid sort_by field. Allowed values: created_at, price_per_kwh, energy_amount, filled_at",
                        Some("sort_by"),
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.page_size as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }

    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            crate::utils::SortOrder::Asc => "ASC",
            crate::utils::SortOrder::Desc => "DESC",
        }
    }

    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("created_at")
    }
}

/// Response for trading orders list
#[derive(Debug, Serialize, ToSchema)]
pub struct TradingOrdersResponse {
    pub data: Vec<TradingOrder>,
    pub pagination: crate::utils::PaginationMeta,
}

/// Response for order creation
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateOrderResponse {
    pub id: Uuid,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub message: String,
}

/// Trading statistics for user
#[derive(Debug, Serialize, ToSchema)]
pub struct TradingStats {
    pub total_orders: i64,
    pub active_orders: i64,
    pub filled_orders: i64,
    pub cancelled_orders: i64,
}

/// Trading market data from blockchain
#[derive(Debug, Serialize, ToSchema)]
pub struct BlockchainMarketData {
    pub authority: String,
    pub active_orders: u64,
    pub total_volume: u64,
    pub total_trades: u64,
    pub market_fee_bps: u16,
    pub clearing_enabled: bool,
    pub created_at: i64,
}

/// Create blockchain order request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBlockchainOrderRequest {
    pub order_type: String, // "buy" or "sell"
    pub energy_amount: u64,
    pub price_per_kwh: u64,
}

/// Create blockchain order response
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateBlockchainOrderResponse {
    pub success: bool,
    pub message: String,
    pub order_type: String,
    pub energy_amount: u64,
    pub price_per_kwh: u64,
    pub transaction_signature: Option<String>,
}

/// Match orders response
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchOrdersResponse {
    pub success: bool,
    pub message: String,
    pub matched_orders: u32,
    pub total_volume: u64,
}

/// Market statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStats {
    pub average_price: f64,
    pub total_volume: f64,
    pub active_orders: i64,
    pub pending_orders: i64,
    pub completed_matches: i64,
}
