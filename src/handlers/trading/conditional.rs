//! Conditional Orders Handler (Stop-Loss/Take-Profit)
//!
//! Handles creation, listing, and cancellation of conditional orders

use axum::{extract::{State, Path}, response::Json};
use chrono::{Utc, Duration};
use rust_decimal::Decimal;
use uuid::Uuid;
use tracing::{info, error};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::trading::{
    CreateConditionalOrderRequest, ConditionalOrderResponse, ConditionalOrder,
    TriggerType, TriggerStatus,
};
use crate::database::schema::types::{OrderSide, OrderType, OrderStatus};
use crate::AppState;

/// Create a new conditional order (stop-loss/take-profit)
/// POST /api/v1/trading/conditional-orders
#[utoipa::path(
    post,
    path = "/api/v1/trading/conditional-orders",
    tag = "trading",
    request_body = CreateConditionalOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Conditional order created", body = ConditionalOrderResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_conditional_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateConditionalOrderRequest>,
) -> Result<Json<ConditionalOrderResponse>> {
    info!("Creating conditional order for user: {}, type: {:?}", user.0.sub, payload.trigger_type);

    // Validate trigger price
    if payload.trigger_price <= Decimal::ZERO {
        return Err(ApiError::BadRequest("Trigger price must be positive".to_string()));
    }

    if payload.energy_amount <= Decimal::ZERO {
        return Err(ApiError::BadRequest("Energy amount must be positive".to_string()));
    }

    // Validate trailing offset for trailing stop orders
    if payload.trigger_type == TriggerType::TrailingStop && payload.trailing_offset.is_none() {
        return Err(ApiError::BadRequest("Trailing offset is required for trailing stop orders".to_string()));
    }

    let order_id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at = payload.expiry_time.unwrap_or_else(|| now + Duration::days(7));
    
    // Determine order type based on limit_price
    let order_type = if payload.limit_price.is_some() {
        OrderType::Limit
    } else {
        OrderType::Market
    };

    // Insert conditional order into database
    // Persist session_token in the database
    let result = sqlx::query(
        r#"
        INSERT INTO trading_orders (
            id, user_id, order_type, side, energy_amount, price_per_kwh,
            filled_amount, status, expires_at, created_at,
            trigger_price, trigger_type, trigger_status, trailing_offset,
            session_token
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(order_id)
    .bind(user.0.sub)
    .bind(order_type as OrderType)
    .bind(payload.side as OrderSide)
    .bind(payload.energy_amount)
    .bind(payload.limit_price.unwrap_or(Decimal::ZERO))
    .bind(Decimal::ZERO)
    .bind(OrderStatus::Pending as OrderStatus)
    .bind(expires_at)
    .bind(now)
    .bind(payload.trigger_price)
    .bind(payload.trigger_type as TriggerType)
    .bind(TriggerStatus::Pending as TriggerStatus)
    .bind(payload.trailing_offset)
    .bind(payload.session_token)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to create conditional order: {}", e);
        ApiError::Internal(format!("Failed to create order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::Internal("Failed to insert order".to_string()));
    }

    info!("Created conditional order {} for user {}", order_id, user.0.sub);

    Ok(Json(ConditionalOrderResponse {
        id: order_id,
        trigger_type: payload.trigger_type,
        trigger_status: TriggerStatus::Pending,
        trigger_price: payload.trigger_price,
        created_at: now,
        message: format!(
            "{} order created. Will trigger when price {} {}",
            payload.trigger_type,
            if payload.trigger_type == TriggerType::StopLoss { "falls below" } else { "rises above" },
            payload.trigger_price
        ),
    }))
}

/// List user's conditional orders
/// GET /api/v1/trading/conditional-orders
#[utoipa::path(
    get,
    path = "/api/v1/trading/conditional-orders",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of conditional orders", body = Vec<ConditionalOrder>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_conditional_orders(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Vec<ConditionalOrder>>> {
    info!("Listing conditional orders for user: {}", user.0.sub);

    let orders = sqlx::query_as!(
        ConditionalOrder,
        r#"
        SELECT 
            id,
            user_id,
            side as "side!: OrderSide",
            energy_amount,
            trigger_price as "trigger_price!",
            trigger_type as "trigger_type!: TriggerType",
            trigger_status as "trigger_status!: TriggerStatus",
            price_per_kwh as limit_price,
            trailing_offset,
            expires_at,
            created_at as "created_at!",
            triggered_at
        FROM trading_orders
        WHERE user_id = $1 AND trigger_type IS NOT NULL
        ORDER BY created_at DESC
        LIMIT 100
        "#,
        user.0.sub
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to list conditional orders: {}", e);
        ApiError::Internal(format!("Failed to list orders: {}", e))
    })?;

    Ok(Json(orders))
}

/// Cancel a conditional order
/// DELETE /api/v1/trading/conditional-orders/:id
#[utoipa::path(
    delete,
    path = "/api/v1/trading/conditional-orders/{id}",
    tag = "trading",
    params(
        ("id" = Uuid, Path, description = "Order ID to cancel")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order cancelled"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Order not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn cancel_conditional_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    info!("Cancelling conditional order {} for user {}", order_id, user.0.sub);

    let result = sqlx::query!(
        r#"
        UPDATE trading_orders
        SET trigger_status = 'cancelled', status = 'cancelled'
        WHERE id = $1 AND user_id = $2 AND trigger_status = 'pending'
        "#,
        order_id,
        user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to cancel conditional order: {}", e);
        ApiError::Internal(format!("Failed to cancel order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Order not found or already triggered/cancelled".to_string()));
    }

    info!("Cancelled conditional order {}", order_id);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Conditional order cancelled successfully",
        "order_id": order_id
    })))
}
