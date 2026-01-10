use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// WebSocket message types for real-time market updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketEvent {
    /// New offer created in the market
    OfferCreated {
        offer_id: String,
        energy_amount: f64,
        price_per_kwh: f64,
        energy_source: String,
        location: String,
        created_by: String,
    },
    /// Offer updated (e.g., status changed)
    OfferUpdated {
        offer_id: String,
        status: String,
        energy_amount: Option<f64>,
    },
    /// New order placed
    OrderCreated {
        order_id: String,
        energy_amount: f64,
        max_price_per_kwh: f64,
        energy_source: Option<String>,
        created_by: String,
    },
    /// Order matched with an offer
    OrderMatched {
        order_id: String,
        offer_id: String,
        transaction_id: String,
        matched_amount: f64,
        price_per_kwh: f64,
    },
    /// Transaction status changed
    TransactionUpdated {
        transaction_id: String,
        status: String,
        buyer_id: String,
        seller_id: String,
    },
    /// Market statistics update
    MarketStats {
        total_active_offers: i64,
        total_pending_orders: i64,
        average_price: f64,
        total_volume_24h: f64,
    },
    /// Order book update (buy side)
    OrderBookBuyUpdate {
        price_levels: Vec<PriceLevel>,
        best_bid: Option<String>,
        timestamp: String,
    },
    /// Order book update (sell side)
    OrderBookSellUpdate {
        price_levels: Vec<PriceLevel>,
        best_ask: Option<String>,
        timestamp: String,
    },
    /// Order book full snapshot
    OrderBookSnapshot {
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        best_bid: Option<String>,
        best_ask: Option<String>,
        mid_price: Option<String>,
        spread: Option<String>,
        timestamp: String,
    },
    /// Trade execution notification
    TradeExecuted {
        trade_id: String,
        buy_order_id: String,
        sell_order_id: String,
        buyer_id: String,
        seller_id: String,
        quantity: String,
        price: String,
        total_value: String,
        executed_at: String,
    },
    /// Market depth update
    MarketDepthUpdate {
        total_buy_volume: String,
        total_sell_volume: String,
        buy_orders_count: usize,
        sell_orders_count: usize,
        spread_percentage: Option<f64>,
    },

    /// Meter reading received event
    MeterReadingReceived {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        power: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        voltage: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        current: Option<f64>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Tokens minted event
    TokensMinted {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        tokens_minted: u64,
        transaction_signature: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Meter reading validation failed event
    MeterReadingValidationFailed {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        error_reason: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Batch minting completed event
    BatchMintingCompleted {
        batch_id: String,
        total_readings: u32,
        successful_mints: u32,
        failed_mints: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Aggregate grid status updated
    GridStatusUpdated {
        total_generation: f64,
        total_consumption: f64,
        net_balance: f64,
        active_meters: i64,
        co2_saved_kg: f64,
        #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
        zones: std::collections::HashMap<i32, ZoneStatus>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Meter alert event
    MeterAlert {
        meter_id: String,
        alert_type: String,
        severity: String,
        message: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneStatus {
    pub zone_id: i32,
    pub generation: f64,
    pub consumption: f64,
    pub net_balance: f64,
    pub active_meters: i32,
}

/// Price level for order book updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub volume: String,
}
