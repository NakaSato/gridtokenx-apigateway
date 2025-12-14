// ! Market clearing types and shared structures

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Order side (Buy or Sell)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub epoch_id: Option<Uuid>,
    pub side: OrderSide,
    pub energy_amount: Decimal, // kWh
    pub price: Decimal,         // USD per kWh
    pub filled_amount: Decimal, // kWh already filled
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl BookOrder {
    /// Remaining unfilled amount
    pub fn remaining_amount(&self) -> Decimal {
        self.energy_amount - self.filled_amount
    }

    /// Check if order is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if order is fully filled
    pub fn is_filled(&self) -> bool {
        self.filled_amount >= self.energy_amount
    }
}

/// Trade match result
#[derive(Debug, Clone, Serialize)]
pub struct TradeMatch {
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    pub price: Decimal,
    pub quantity: Decimal,
    pub total_value: Decimal,
    pub matched_at: DateTime<Utc>,
    pub epoch_id: Uuid,
}

/// Market clearing price calculation result
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClearingPrice {
    #[schema(value_type = String)]
    pub price: Decimal,
    #[schema(value_type = String)]
    pub volume: Decimal,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
}

/// Order book snapshot for API responses
#[derive(Debug, Clone, Serialize)]
pub struct OrderBookSnapshot {
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub mid_price: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub buy_depth: Vec<(Decimal, Decimal)>, // (price, volume)
    pub sell_depth: Vec<(Decimal, Decimal)>,
    pub timestamp: DateTime<Utc>,
}
