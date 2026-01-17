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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MarketStats {
    pub average_price: f64,
    pub total_volume: f64,
    pub active_orders: i64,
    pub pending_orders: i64,
    pub completed_matches: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderBookEntry {
    pub energy_amount: f64,
    pub price_per_kwh: f64,
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderBookResponse {
    pub buy_orders: Vec<OrderBookEntry>,
    pub sell_orders: Vec<OrderBookEntry>,
    pub timestamp: DateTime<Utc>,
}

// =============================================================================
// P2P Transaction Types
// =============================================================================

/// Request for calculating P2P transaction cost
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct P2PCalculateCostRequest {
    /// Buyer's zone ID
    pub buyer_zone_id: i32,

    /// Seller's zone ID
    pub seller_zone_id: i32,

    /// Amount of energy to trade in kWh
    #[validate(range(min = 0.001, message = "Energy amount must be positive"))]
    pub energy_amount: f64,

    /// Negotiated price per kWh in THB (optional, defaults to market base price)
    pub agreed_price: Option<f64>,
}

/// Complete breakdown of a P2P energy transaction cost
#[derive(Debug, Serialize, ToSchema)]
pub struct P2PTransactionCost {
    /// Base energy price × amount (THB)
    pub energy_cost: f64,

    /// Zone-based transmission fee (THB)
    pub wheeling_charge: f64,

    /// Monetized energy loss (THB)
    pub loss_cost: f64,

    /// Sum of all costs (THB)
    pub total_cost: f64,

    /// Energy received after losses (kWh)
    pub effective_energy: f64,

    /// Loss percentage applied (decimal)
    pub loss_factor: f64,

    /// Loss payment model ("RECEIVER" or "SENDER")
    pub loss_allocation: String,

    /// Distance between zones (km)
    pub zone_distance_km: f64,

    /// Buyer's zone ID
    pub buyer_zone: i32,

    /// Seller's zone ID
    pub seller_zone: i32,

    /// Whether transaction complies with grid constraints
    pub is_grid_compliant: bool,

    /// Reason for grid violation (if any)
    pub grid_violation_reason: Option<String>,
}

/// Market pricing configuration
#[derive(Debug, Serialize, ToSchema)]
pub struct P2PMarketPrices {
    /// Base P2P energy price in THB/kWh
    pub base_price_thb_kwh: f64,

    /// Price when buying from main grid in THB/kWh
    pub grid_import_price_thb_kwh: f64,

    /// FiT rate when selling to main grid in THB/kWh
    pub grid_export_price_thb_kwh: f64,

    /// Loss payment model ("RECEIVER" or "SENDER")
    pub loss_allocation_model: String,

    /// Zone-based wheeling charges (e.g., "intra_zone": 0.5)
    pub wheeling_charges: std::collections::HashMap<String, f64>,

    /// Zone-based loss factors (e.g., "intra_zone": 0.01)
    pub loss_factors: std::collections::HashMap<String, f64>,
}

/// P2P vs Grid price comparison
#[derive(Debug, Serialize, ToSchema)]
pub struct P2PGridComparison {
    /// P2P transaction cost breakdown
    pub p2p_transaction: P2PTransactionCost,

    /// Cost if buying from grid (THB)
    pub grid_import_cost: f64,

    /// Value if selling to grid (THB)
    pub grid_export_value: f64,

    /// Buyer savings with P2P vs grid (THB)
    pub buyer_savings_thb: f64,

    /// Seller premium with P2P vs grid export (THB)
    pub seller_premium_thb: f64,

    /// Is P2P beneficial for buyer?
    pub is_p2p_beneficial_for_buyer: bool,

    /// Is P2P beneficial for seller?
    pub is_p2p_beneficial_for_seller: bool,
}

