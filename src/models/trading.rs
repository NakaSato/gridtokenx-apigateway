use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TradingOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub order_type: OrderType,
    pub side: OrderSide,
    #[schema(value_type = String)]
    pub energy_amount: Decimal,
    #[schema(value_type = String)]
    pub price_per_kwh: Decimal,
    #[schema(value_type = String)]
    pub filled_amount: Decimal,
    pub status: OrderStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub filled_at: Option<DateTime<Utc>>,
    pub epoch_id: Option<Uuid>,
    pub zone_id: Option<i32>,
    pub meter_id: Option<Uuid>,
    pub refund_tx_signature: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TradingOrderDb {
    pub id: Uuid,
    pub user_id: Uuid,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub energy_amount: Decimal,
    pub price_per_kwh: Decimal,
    pub filled_amount: Option<Decimal>,
    pub status: OrderStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub filled_at: Option<DateTime<Utc>>,
    pub epoch_id: Option<Uuid>,
    pub zone_id: Option<i32>,
    pub meter_id: Option<Uuid>,
    pub refund_tx_signature: Option<String>,
}

impl From<TradingOrderDb> for TradingOrder {
    fn from(db: TradingOrderDb) -> Self {
        Self {
            id: db.id,
            user_id: db.user_id,
            order_type: db.order_type,
            side: db.side,
            energy_amount: db.energy_amount,
            price_per_kwh: db.price_per_kwh,
            filled_amount: db.filled_amount.unwrap_or(Decimal::ZERO),
            status: db.status,
            expires_at: db.expires_at,
            created_at: db.created_at,
            filled_at: db.filled_at,
            epoch_id: db.epoch_id,
            zone_id: db.zone_id,
            meter_id: db.meter_id,
            refund_tx_signature: db.refund_tx_signature,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct EscrowRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub order_id: Option<Uuid>,
    #[schema(value_type = String)]
    pub amount: Decimal,
    pub asset_type: String, // 'currency', 'energy'
    pub escrow_type: String, // 'buy_lock', 'sell_lock'
    pub status: String, // 'locked', 'released', 'refunded'
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOrderRequest {
    pub side: OrderSide,
    
    #[schema(value_type = String, example = "10.5")]
    pub energy_amount: Decimal,
    
    #[schema(value_type = String, example = "0.15")]
    pub price_per_kwh: Option<Decimal>,

    pub order_type: OrderType,

    pub expiry_time: Option<DateTime<Utc>>,

    pub zone_id: Option<i32>,

    pub meter_id: Option<Uuid>,

    /// HMAC-SHA256 signature of the order parameters
    pub signature: Option<String>,
    
    /// Timestamp of when the signature was created
    pub timestamp: Option<i64>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateOrderRequest {
    #[schema(value_type = String)]
    pub energy_amount: Option<Decimal>,
    #[schema(value_type = String)]
    pub price_per_kwh: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MarketData {
    pub current_epoch: u64,
    pub epoch_start_time: DateTime<Utc>,
    pub epoch_end_time: DateTime<Utc>,
    pub status: String,
    pub order_book: OrderBook,
    pub recent_trades: Vec<Trade>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrderBook {
    pub sell_orders: Vec<TradingOrder>,
    pub buy_orders: Vec<TradingOrder>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Trade {
    pub id: Uuid,
    #[schema(value_type = String)]
    pub price: Decimal,
    #[schema(value_type = String)]
    pub amount: Decimal,
    pub executed_at: DateTime<Utc>,
}
