
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use rust_decimal::Decimal;
use crate::error::{ApiError, Result};

#[derive(Debug, Clone)]
pub struct P2PService {
    db: PgPool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "p2p_order_side", rename_all = "lowercase")]
pub enum P2POrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "p2p_order_status", rename_all = "lowercase")]
pub enum P2POrderStatus {
    Open,
    Filled,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2POrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub side: Option<String>, 
    pub amount: Option<Decimal>,
    pub price_per_kwh: Option<Decimal>,
    pub filled_amount: Option<Decimal>,
    pub status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_email: Option<String>, // Join with users table usually
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub side: P2POrderSide,
    pub amount: Decimal,
    pub price_per_kwh: Decimal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBookResponse {
    pub asks: Vec<P2POrder>,
    pub bids: Vec<P2POrder>,
}

impl P2PService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn create_order(&self, user_id: Uuid, req: CreateOrderRequest) -> Result<Uuid> {
        let order_id = sqlx::query!(
            r#"
            INSERT INTO p2p_orders (user_id, side, amount, price_per_kwh)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            user_id,
            req.side as P2POrderSide,
            req.amount,
            req.price_per_kwh
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .id;

        Ok(order_id)
    }

    pub async fn get_order_book(&self) -> Result<OrderBookResponse> {
        let orders = sqlx::query_as!(
            P2POrder,
            r#"
            SELECT 
                o.id, o.user_id, 
                COALESCE(o.side::text, 'unknown') as side, 
                COALESCE(o.amount, 0) as amount, 
                COALESCE(o.price_per_kwh, 0) as price_per_kwh, 
                COALESCE(o.filled_amount, 0) as filled_amount, 
                COALESCE(o.status::text, 'unknown') as status,
                o.created_at, o.updated_at,
                u.email as user_email
            FROM p2p_orders o
            JOIN users u ON o.user_id = u.id
            WHERE o.status = 'open'
            ORDER BY o.created_at DESC
            "#
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

        let mut asks = Vec::new();
        let mut bids = Vec::new();

        for order in orders {
            if order.side.as_deref() == Some("buy") {
                bids.push(order);
            } else {
                asks.push(order);
            }
        }

        Ok(OrderBookResponse { asks, bids })
    }

    pub async fn get_user_orders(&self, user_id: Uuid) -> Result<Vec<P2POrder>> {
        let orders = sqlx::query_as!(
            P2POrder,
            r#"
            SELECT 
                o.id, o.user_id, 
                COALESCE(o.side::text, 'unknown') as side, 
                COALESCE(o.amount, 0) as amount, 
                COALESCE(o.price_per_kwh, 0) as price_per_kwh, 
                COALESCE(o.filled_amount, 0) as filled_amount, 
                COALESCE(o.status::text, 'unknown') as status,
                o.created_at, o.updated_at,
                u.email as user_email
            FROM p2p_orders o
            JOIN users u ON o.user_id = u.id
            WHERE o.user_id = $1
            ORDER BY o.created_at DESC
            LIMIT 50
            "#,
            user_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

        Ok(orders)
    }
}
