use serde::Serialize;
use utoipa::ToSchema;

/// Market statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStats {
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub mid_price: Option<String>,
    pub spread: Option<String>,
    pub spread_percentage: Option<f64>,
    pub total_buy_volume: String,
    pub total_sell_volume: String,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
}

/// Order book depth response
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookDepth {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub mid_price: Option<String>,
    pub spread: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PriceLevel {
    pub price: String,
    pub volume: String,
}

/// User's recent trades
#[derive(Debug, Serialize, ToSchema)]
pub struct TradeHistory {
    pub trades: Vec<TradeRecord>,
    pub total_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TradeRecord {
    pub id: String,
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub quantity: String,
    pub price: String,
    pub total_value: String,
    pub role: String, // "buyer" or "seller"
    pub counterparty_id: String,
    pub executed_at: String,
    pub status: String,
}

/// Market depth chart data
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketDepthChart {
    pub cumulative_bids: Vec<DepthPoint>,
    pub cumulative_asks: Vec<DepthPoint>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DepthPoint {
    pub price: String,
    pub cumulative_volume: String,
}
