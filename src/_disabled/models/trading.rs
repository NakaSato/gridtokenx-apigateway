use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TradingOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub epoch_id: Option<Uuid>,
    pub order_type: OrderType,
    pub side: OrderSide,
    #[schema(value_type = f64)]
    pub energy_amount: Decimal,
    #[schema(value_type = f64)]
    pub price_per_kwh: Decimal,
    #[schema(value_type = f64)]
    pub filled_amount: Decimal,
    pub status: OrderStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub filled_at: Option<DateTime<Utc>>,
}

// Internal database model with Decimal for database operations
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TradingOrderDb {
    pub id: Uuid,
    pub user_id: Uuid,
    pub epoch_id: Option<Uuid>,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub energy_amount: Decimal,
    pub price_per_kwh: Decimal,
    pub filled_amount: Decimal,
    pub status: OrderStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub filled_at: Option<DateTime<Utc>>,
}

impl From<TradingOrderDb> for TradingOrder {
    fn from(db_order: TradingOrderDb) -> Self {
        TradingOrder {
            id: db_order.id,
            user_id: db_order.user_id,
            epoch_id: db_order.epoch_id,
            order_type: db_order.order_type,
            side: db_order.side,
            energy_amount: db_order.energy_amount,
            price_per_kwh: db_order.price_per_kwh,
            filled_amount: db_order.filled_amount,
            status: db_order.status,
            expires_at: db_order.expires_at,
            created_at: db_order.created_at,
            filled_at: db_order.filled_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateOrderRequest {
    #[schema(value_type = f64)]
    pub energy_amount: rust_decimal::Decimal,
    #[schema(value_type = Option<f64>)]
    pub price_per_kwh: Option<rust_decimal::Decimal>,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub expiry_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MarketData {
    pub current_epoch: u64,
    pub epoch_start_time: DateTime<Utc>,
    pub epoch_end_time: DateTime<Utc>,
    pub status: String,
    pub order_book: OrderBook,
    pub recent_trades: Vec<TradeExecution>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderBook {
    pub sell_orders: Vec<TradingOrder>,
    pub buy_orders: Vec<TradingOrder>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TradeExecution {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    #[schema(value_type = f64)]
    pub energy_amount: rust_decimal::Decimal,
    #[schema(value_type = f64)]
    pub price_per_kwh: rust_decimal::Decimal,
    #[schema(value_type = f64)]
    pub total_price: rust_decimal::Decimal,
    pub executed_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateOrderRequest {
    #[schema(value_type = Option<f64>)]
    pub energy_amount: Option<rust_decimal::Decimal>,
    #[schema(value_type = Option<f64>)]
    pub price_per_kwh: Option<rust_decimal::Decimal>,
}
