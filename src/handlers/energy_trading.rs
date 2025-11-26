use axum::{
    extract::{Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::types::BigDecimal;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::OrderSide;
use crate::error::{ApiError, Result};
use crate::utils::{PaginationMeta, PaginationParams, SortOrder, validation::Validator};

// Helper to parse BigDecimal from string
use std::str::FromStr;

// ==================== REQUEST/RESPONSE TYPES ====================

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOrderRequest {
    #[validate(range(min = 0.001, max = 100000.0))]
    pub energy_amount: f64, // kWh
    #[validate(range(min = 0.01, max = 10.00))]
    pub price_per_kwh: f64, // USD
    pub order_type: String, // "buy" or "sell"
}

impl CreateOrderRequest {
    pub fn validate_business_rules(&self) -> Result<()> {
        // Validate order type
        if !["buy", "sell"].contains(&self.order_type.as_str()) {
            return Err(ApiError::validation_field(
                "order_type",
                "order_type must be 'buy' or 'sell'",
            ));
        }

        // Validate energy amount
        Validator::validate_energy_reading(self.energy_amount)?;

        // Validate price
        Validator::validate_price(self.price_per_kwh)?;

        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TradingOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: Option<String>,
    pub order_type: String,
    pub energy_amount: f64,
    pub price_per_kwh: f64,
    pub filled_amount: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderMatch {
    pub id: Uuid,
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub matched_amount: f64,
    pub match_price: f64,
    pub match_time: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStats {
    pub average_price: f64,
    pub total_volume: f64,
    pub active_orders: i64,
    pub pending_orders: i64,
    pub completed_matches: i64,
}

#[derive(Debug, Deserialize, Validate, IntoParams)]
pub struct OrderQuery {
    pub order_type: Option<String>,
    pub status: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_amount: Option<f64>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
fn default_sort_order() -> SortOrder {
    SortOrder::Desc
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrdersResponse {
    pub data: Vec<TradingOrder>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderMatchesResponse {
    pub data: Vec<OrderMatch>,
    pub pagination: PaginationMeta,
}

// ==================== ORDER ENDPOINTS ====================

/// Create a new trading order
#[utoipa::path(
    post,
    path = "/api/trading/orders",
    tag = "energy-trading",
    request_body = CreateOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order created successfully", body = TradingOrder),
        (status = 400, description = "Invalid order data"),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn create_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<TradingOrder>> {
    // Validate struct constraints
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Validate business rules
    payload.validate_business_rules()?;

    let now = Utc::now();
    let order_id = Uuid::new_v4();

    // Convert to BigDecimal for database
    let energy_amount_bd = BigDecimal::from_str(&payload.energy_amount.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid energy amount".to_string()))?;
    let price_bd = BigDecimal::from_str(&payload.price_per_kwh.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid price".to_string()))?;

    // Determine order side based on payload
    let order_side = match payload.order_type.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(ApiError::BadRequest("Invalid order type".to_string())),
    };

    // Get or create current market epoch
    let epoch = state
        .market_clearing_service
        .get_or_create_epoch(Utc::now())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get/create epoch: {}", e);
            ApiError::Internal(e.to_string())
        })?;

    // Set expiration to 24 hours from now
    let expires_at = now + chrono::Duration::hours(24);

    sqlx::query(
        r#"
        INSERT INTO trading_orders (
            id, user_id, order_type, side, energy_amount, price_per_kwh,
            filled_amount, status, created_at, updated_at, epoch_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, 0, 'pending', $7, $7, $8, $9)
        "#,
    )
    .bind(order_id)
    .bind(user.0.sub)
    .bind("limit")
    .bind(order_side as OrderSide)
    .bind(energy_amount_bd)
    .bind(price_bd)
    .bind(now)
    .bind(epoch.id)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create trading order: {}", e);
        ApiError::Database(e)
    })?;

    // Trigger order matching engine
    tokio::spawn({
        let engine = state.order_matching_engine.clone();
        async move {
            if let Err(e) = engine.trigger_matching().await {
                tracing::error!("Failed to trigger matching after order creation: {}", e);
            }
        }
    });

    Ok(Json(TradingOrder {
        id: order_id,
        user_id: user.0.sub,
        username: Some(user.0.username.clone()),
        order_type: payload.order_type,
        energy_amount: payload.energy_amount,
        price_per_kwh: payload.price_per_kwh,
        filled_amount: 0.0,
        status: "pending".to_string(),
        created_at: now,
    }))
}

/// List trading orders with pagination and filters
#[utoipa::path(
    get,
    path = "/api/trading/orders",
    tag = "energy-trading",
    params(OrderQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of trading orders", body = OrdersResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn list_orders(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(params): Query<OrderQuery>,
) -> Result<Json<OrdersResponse>> {
    let limit = 20i64;
    let offset = 0i64;

    // Build WHERE conditions
    let mut where_conditions = vec!["1=1".to_string()];

    if let Some(order_type) = &params.order_type {
        where_conditions.push(format!("order_type = '{}'", order_type));
    }
    if let Some(status) = &params.status {
        where_conditions.push(format!("o.status::TEXT = '{}'", status));
    }
    if let Some(min_price) = params.min_price {
        where_conditions.push(format!("price_per_kwh >= {}", min_price));
    }
    if let Some(max_price) = params.max_price {
        where_conditions.push(format!("price_per_kwh <= {}", max_price));
    }
    if let Some(min_amount) = params.min_amount {
        where_conditions.push(format!("energy_amount >= {}", min_amount));
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM trading_orders WHERE {}", where_clause);
    let total = sqlx::query_scalar::<_, i64>(&count_query)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count orders: {}", e);
            ApiError::Database(e)
        })?;

    // Build data query
    let data_query = format!(
        r#"
        SELECT o.id, o.user_id, u.username, o.order_type, 
               o.energy_amount, o.price_per_kwh, o.filled_amount,
               o.status::TEXT as status, o.created_at
        FROM trading_orders o
        JOIN users u ON o.user_id = u.id
        WHERE {}
        ORDER BY o.created_at DESC
        LIMIT {} OFFSET {}
        "#,
        where_clause, limit, offset
    );

    let rows = sqlx::query(&data_query)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch orders: {}", e);
            ApiError::Database(e)
        })?;

    let orders = rows
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let price_per_kwh: BigDecimal = row.get("price_per_kwh");
            let filled_amount: BigDecimal = row.get("filled_amount");

            TradingOrder {
                id: row.get("id"),
                user_id: row.get("user_id"),
                username: row.get("username"),
                order_type: row.get("order_type"),
                energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
                price_per_kwh: price_per_kwh.to_string().parse().unwrap_or(0.0),
                filled_amount: filled_amount.to_string().parse().unwrap_or(0.0),
                status: row.get("status"),
                created_at: row.get("created_at"),
            }
        })
        .collect();

    // Create pagination metadata
    let pagination = PaginationMeta::new(
        &PaginationParams {
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_order: SortOrder::Desc,
        },
        total,
    );

    Ok(Json(OrdersResponse {
        data: orders,
        pagination,
    }))
}

/// Get order book (buy and sell orders)
#[utoipa::path(
    get,
    path = "/api/trading/orderbook",
    tag = "energy-trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order book data"),
    )
)]
pub async fn get_orderbook(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    // Get buy orders
    let buy_orders = sqlx::query(
        r#"
        SELECT o.energy_amount, o.price_per_kwh, u.username
        FROM trading_orders o
        JOIN users u ON o.user_id = u.id
        WHERE o.order_type = 'buy' AND o.status::TEXT = 'pending'
        ORDER BY o.price_per_kwh DESC, o.created_at ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    // Get sell orders
    let sell_orders = sqlx::query(
        r#"
        SELECT o.energy_amount, o.price_per_kwh, u.username
        FROM trading_orders o
        JOIN users u ON o.user_id = u.id
        WHERE o.order_type = 'sell' AND o.status::TEXT = 'pending'
        ORDER BY o.price_per_kwh ASC, o.created_at ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let buys: Vec<serde_json::Value> = buy_orders
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let price_per_kwh: BigDecimal = row.get("price_per_kwh");
            serde_json::json!({
                "energy_amount": energy_amount.to_string().parse::<f64>().unwrap_or(0.0),
                "price_per_kwh": price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0),
                "username": row.get::<Option<String>, _>("username")
            })
        })
        .collect::<Vec<_>>();

    let sells: Vec<serde_json::Value> = sell_orders
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let price_per_kwh: BigDecimal = row.get("price_per_kwh");
            serde_json::json!({
                "energy_amount": energy_amount.to_string().parse::<f64>().unwrap_or(0.0),
                "price_per_kwh": price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0),
                "username": row.get::<Option<String>, _>("username")
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "buy_orders": buys,
        "sell_orders": sells,
        "timestamp": Utc::now()
    })))
}

/// Get market statistics
#[utoipa::path(
    get,
    path = "/api/trading/stats",
    tag = "energy-trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Market statistics", body = MarketStats),
    )
)]
pub async fn get_market_stats(State(state): State<AppState>) -> Result<Json<MarketStats>> {
    // Get average price from recent matches
    let stats_row = sqlx::query(
        r#"
        SELECT
            COALESCE(AVG(match_price), 0) as avg_price,
            COALESCE(SUM(matched_amount), 0) as total_volume,
            COUNT(*) as completed_matches
        FROM order_matches
        WHERE created_at > NOW() - INTERVAL '24 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let avg_price: BigDecimal = stats_row
        .try_get("avg_price")
        .unwrap_or(BigDecimal::from_str("0").unwrap());
    let total_volume: BigDecimal = stats_row
        .try_get("total_volume")
        .unwrap_or(BigDecimal::from_str("0").unwrap());
    let completed_matches: i64 = stats_row.try_get("completed_matches").unwrap_or(0);

    // Get active orders count
    let active_orders_row =
        sqlx::query("SELECT COUNT(*) as count FROM trading_orders WHERE status::TEXT = 'active'")
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Database(e))?;
    let active_orders: i64 = active_orders_row.try_get("count").unwrap_or(0);

    // Get pending orders count
    let pending_orders_row =
        sqlx::query("SELECT COUNT(*) as count FROM trading_orders WHERE status::TEXT = 'pending'")
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Database(e))?;
    let pending_orders: i64 = pending_orders_row.try_get("count").unwrap_or(0);

    Ok(Json(MarketStats {
        average_price: avg_price.to_string().parse().unwrap_or(0.0),
        total_volume: total_volume.to_string().parse().unwrap_or(0.0),
        active_orders,
        pending_orders,
        completed_matches,
    }))
}
