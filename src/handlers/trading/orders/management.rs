use axum::{
    extract::{Path, State},
    response::Json,
};
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::trading::{CreateOrderRequest, TradingOrder};
use crate::AppState;

/// Cancel a trading order
#[utoipa::path(
    delete,
    path = "/api/trading/orders/{id}",
    tag = "trading",
    security(("bearer_auth" = [])),
    params(
        ("id" = Uuid, Path, description = "Order ID to cancel")
    ),
    responses(
        (status = 200, description = "Order cancelled successfully", body = TradingOrder),
        (status = 404, description = "Order not found"),
        (status = 400, description = "Order cannot be cancelled")
    )
)]
pub async fn cancel_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<TradingOrder>> {
    // 1. Check if order exists and belongs to user
    let order = sqlx::query_as::<_, crate::models::trading::TradingOrderDb>(
        "SELECT * FROM trading_orders WHERE id = $1 AND user_id = $2",
    )
    .bind(order_id)
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let order = match order {
        Some(o) => o,
        None => return Err(ApiError::NotFound(format!("Order {} not found", order_id))),
    };

    // 2. Validate status
    // Only pending orders can be cancelled
    if order.status != crate::database::schema::types::OrderStatus::Pending {
        return Err(ApiError::BadRequest(format!(
            "Cannot cancel order with status: {}",
            order.status
        )));
    }

    // 3. Update status to cancelled
    let updated_order = sqlx::query_as::<_, crate::models::trading::TradingOrderDb>(
        "UPDATE trading_orders SET status = 'cancelled', updated_at = NOW() WHERE id = $1 RETURNING *"
    )
    .bind(order_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // 4. Refund Escrow for remaining portion
    use rust_decimal::Decimal;
    use crate::database::schema::types::OrderSide;
    
    let remaining_amount = updated_order.energy_amount - updated_order.filled_amount.unwrap_or(Decimal::ZERO);
    if remaining_amount > Decimal::ZERO {
        match updated_order.side {
            OrderSide::Buy => {
                let refund_value = remaining_amount * updated_order.price_per_kwh;
                if let Err(e) = state.market_clearing.unlock_funds(user.0.sub, order_id, refund_value, "Order Cancelled").await {
                    tracing::error!("Failed to refund funds for cancelled order {}: {}", order_id, e);
                }
            }
            OrderSide::Sell => {
                if let Err(e) = state.market_clearing.unlock_energy(user.0.sub, order_id, remaining_amount, "Order Cancelled").await {
                    tracing::error!("Failed to unlock energy for cancelled order {}: {}", order_id, e);
                }
            }
        }
    }

    // 5. Return updated order
    Ok(Json(updated_order.into()))
}

/// Update a trading order
#[utoipa::path(
    put,
    path = "/api/trading/orders/{id}",
    tag = "trading",
    request_body = CreateOrderRequest,
    security(("bearer_auth" = [])),
    params(
        ("id" = Uuid, Path, description = "Order ID to update")
    ),
    responses(
        (status = 200, description = "Order updated successfully", body = TradingOrder),
        (status = 404, description = "Order not found"),
        (status = 400, description = "Order cannot be updated (not pending or validation failed)")
    )
)]
pub async fn update_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
    Json(payload): Json<crate::models::trading::UpdateOrderRequest>,
) -> Result<Json<TradingOrder>> {
    // 1. Validate payload
    if let Some(amt) = payload.energy_amount {
        if amt <= rust_decimal::Decimal::ZERO {
            return Err(ApiError::BadRequest(
                "Energy amount must be positive".to_string(),
            ));
        }
    }
    if let Some(price) = payload.price_per_kwh {
        if price <= rust_decimal::Decimal::ZERO {
            return Err(ApiError::BadRequest("Price must be positive".to_string()));
        }
    }

    // 2. Fetch order
    let order = sqlx::query_as::<_, crate::models::trading::TradingOrderDb>(
        "SELECT * FROM trading_orders WHERE id = $1 AND user_id = $2",
    )
    .bind(order_id)
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let order = match order {
        Some(o) => o,
        None => return Err(ApiError::NotFound(format!("Order {} not found", order_id))),
    };

    // 3. Validate status
    if order.status != crate::database::schema::types::OrderStatus::Pending {
        return Err(ApiError::BadRequest(
            "Only pending orders can be updated".to_string(),
        ));
    }

    // 4. Update fields
    let new_energy = payload.energy_amount.unwrap_or(order.energy_amount);
    let new_price = payload.price_per_kwh.unwrap_or(order.price_per_kwh);

    // 5. Adjust Escrow
    use crate::database::schema::types::OrderSide;
    match order.side {
        OrderSide::Buy => {
            let old_escrow = order.energy_amount * order.price_per_kwh;
            let new_escrow = new_energy * new_price;
            if new_escrow > old_escrow {
                if let Err(e) = state.market_clearing.lock_funds(user.0.sub, order_id, new_escrow - old_escrow).await {
                    return Err(ApiError::BadRequest(format!("Insufficient balance for update: {}", e)));
                }
            } else if new_escrow < old_escrow {
                if let Err(e) = state.market_clearing.unlock_funds(user.0.sub, order_id, old_escrow - new_escrow, "Order Updated").await {
                    tracing::error!("Failed to adjust escrow for updated order {}: {}", order_id, e);
                }
            }
        }
        OrderSide::Sell => {
            if new_energy > order.energy_amount {
                if let Err(e) = state.market_clearing.lock_energy(user.0.sub, order_id, new_energy - order.energy_amount).await {
                    return Err(ApiError::Internal(format!("Energy lock failed: {}", e)));
                }
            } else if new_energy < order.energy_amount {
                if let Err(e) = state.market_clearing.unlock_energy(user.0.sub, order_id, order.energy_amount - new_energy, "Order Updated").await {
                    tracing::error!("Failed to adjust energy lock for updated order {}: {}", order_id, e);
                }
            }
        }
    }

    // 6. Update DB
    let updated_order = sqlx::query_as::<_, crate::models::trading::TradingOrderDb>(
        r#"
        UPDATE trading_orders 
        SET energy_amount = $1, price_per_kwh = $2, updated_at = NOW()
        WHERE id = $3
        RETURNING *
        "#,
    )
    .bind(new_energy)
    .bind(new_price)
    .bind(order_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // 6. Return updated order
    Ok(Json(updated_order.into()))
}
