//! Recurring Orders Handler (DCA)
//!
//! Handles creation, listing, updating, pausing, resuming, and cancellation of recurring orders

use axum::{extract::{State, Path}, response::Json};
use chrono::{Utc, Duration};
use rust_decimal::Decimal;
use uuid::Uuid;
use tracing::{info, error};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::trading::{
    CreateRecurringOrderRequest, UpdateRecurringOrderRequest,
    RecurringOrderResponse, RecurringOrder,
    IntervalType, RecurringStatus,
};
use crate::database::schema::types::OrderSide;
use crate::AppState;

/// Calculate next execution time based on interval
fn calculate_next_execution(interval_type: IntervalType, interval_value: i32) -> chrono::DateTime<Utc> {
    let now = Utc::now();
    match interval_type {
        IntervalType::Hourly => now + Duration::hours(interval_value as i64),
        IntervalType::Daily => now + Duration::days(interval_value as i64),
        IntervalType::Weekly => now + Duration::weeks(interval_value as i64),
        IntervalType::Monthly => now + Duration::days(30 * interval_value as i64), // Approximate
    }
}

/// Create a new recurring order
/// POST /api/v1/trading/recurring-orders
#[utoipa::path(
    post,
    path = "/api/v1/trading/recurring-orders",
    tag = "trading",
    request_body = CreateRecurringOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Recurring order created", body = RecurringOrderResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_recurring_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateRecurringOrderRequest>,
) -> Result<Json<RecurringOrderResponse>> {
    info!("Creating recurring order for user: {}, interval: {:?}", user.0.sub, payload.interval_type);

    if payload.energy_amount <= Decimal::ZERO {
        return Err(ApiError::BadRequest("Energy amount must be positive".to_string()));
    }

    let order_id = Uuid::new_v4();
    let now = Utc::now();
    let interval_value = payload.interval_value.unwrap_or(1);
    let next_execution = calculate_next_execution(payload.interval_type, interval_value);

    let result = sqlx::query!(
        r#"
        INSERT INTO recurring_orders (
            id, user_id, side, energy_amount, max_price_per_kwh, min_price_per_kwh,
            interval_type, interval_value, next_execution_at, status,
            max_executions, name, description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14)
        "#,
        order_id,
        user.0.sub,
        payload.side as OrderSide,
        payload.energy_amount,
        payload.max_price_per_kwh,
        payload.min_price_per_kwh,
        payload.interval_type as IntervalType,
        interval_value,
        next_execution,
        RecurringStatus::Active as RecurringStatus,
        payload.max_executions,
        payload.name,
        payload.description,
        now
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to create recurring order: {}", e);
        ApiError::Internal(format!("Failed to create order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::Internal("Failed to insert order".to_string()));
    }

    info!("Created recurring order {} for user {}", order_id, user.0.sub);

    Ok(Json(RecurringOrderResponse {
        id: order_id,
        status: RecurringStatus::Active,
        next_execution_at: next_execution,
        created_at: now,
        message: format!(
            "Recurring {} order created. First execution at {}",
            payload.side, next_execution.format("%Y-%m-%d %H:%M UTC")
        ),
    }))
}

/// List user's recurring orders
/// GET /api/v1/trading/recurring-orders
#[utoipa::path(
    get,
    path = "/api/v1/trading/recurring-orders",
    tag = "trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of recurring orders", body = Vec<RecurringOrder>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_recurring_orders(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Vec<RecurringOrder>>> {
    info!("Listing recurring orders for user: {}", user.0.sub);

    let orders = sqlx::query_as!(
        RecurringOrder,
        r#"
        SELECT 
            id, user_id, side as "side!: OrderSide",
            energy_amount, max_price_per_kwh, min_price_per_kwh,
            interval_type as "interval_type!: IntervalType",
            interval_value as "interval_value!",
            next_execution_at as "next_execution_at!",
            last_executed_at,
            status as "status!: RecurringStatus",
            total_executions as "total_executions!",
            max_executions,
            name, description,
            created_at as "created_at!",
            updated_at as "updated_at!"
        FROM recurring_orders
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
        user.0.sub
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to list recurring orders: {}", e);
        ApiError::Internal(format!("Failed to list orders: {}", e))
    })?;

    Ok(Json(orders))
}

/// Get a specific recurring order
/// GET /api/v1/trading/recurring-orders/:id
#[utoipa::path(
    get,
    path = "/api/v1/trading/recurring-orders/{id}",
    tag = "trading",
    params(("id" = Uuid, Path, description = "Order ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Recurring order details", body = RecurringOrder),
        (status = 404, description = "Order not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_recurring_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<RecurringOrder>> {
    let order = sqlx::query_as!(
        RecurringOrder,
        r#"
        SELECT 
            id, user_id, side as "side!: OrderSide",
            energy_amount, max_price_per_kwh, min_price_per_kwh,
            interval_type as "interval_type!: IntervalType",
            interval_value as "interval_value!",
            next_execution_at as "next_execution_at!",
            last_executed_at,
            status as "status!: RecurringStatus",
            total_executions as "total_executions!",
            max_executions,
            name, description,
            created_at as "created_at!",
            updated_at as "updated_at!"
        FROM recurring_orders
        WHERE id = $1 AND user_id = $2
        "#,
        order_id,
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to get recurring order: {}", e);
        ApiError::Internal(format!("Failed to get order: {}", e))
    })?
    .ok_or_else(|| ApiError::NotFound("Recurring order not found".to_string()))?;

    Ok(Json(order))
}

/// Cancel a recurring order
/// DELETE /api/v1/trading/recurring-orders/:id
#[utoipa::path(
    delete,
    path = "/api/v1/trading/recurring-orders/{id}",
    tag = "trading",
    params(("id" = Uuid, Path, description = "Order ID to cancel")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order cancelled"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Order not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn cancel_recurring_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    info!("Cancelling recurring order {} for user {}", order_id, user.0.sub);

    let result = sqlx::query!(
        r#"
        UPDATE recurring_orders
        SET status = 'cancelled', updated_at = NOW()
        WHERE id = $1 AND user_id = $2 AND status NOT IN ('cancelled', 'completed')
        "#,
        order_id,
        user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to cancel recurring order: {}", e);
        ApiError::Internal(format!("Failed to cancel order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Order not found or already cancelled/completed".to_string()));
    }

    info!("Cancelled recurring order {}", order_id);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Recurring order cancelled successfully",
        "order_id": order_id
    })))
}

/// Pause a recurring order
/// POST /api/v1/trading/recurring-orders/:id/pause
#[utoipa::path(
    post,
    path = "/api/v1/trading/recurring-orders/{id}/pause",
    tag = "trading",
    params(("id" = Uuid, Path, description = "Order ID to pause")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order paused"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Order not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn pause_recurring_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    info!("Pausing recurring order {} for user {}", order_id, user.0.sub);

    let result = sqlx::query!(
        r#"
        UPDATE recurring_orders
        SET status = 'paused', updated_at = NOW()
        WHERE id = $1 AND user_id = $2 AND status = 'active'
        "#,
        order_id,
        user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to pause recurring order: {}", e);
        ApiError::Internal(format!("Failed to pause order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Order not found or not active".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Recurring order paused",
        "order_id": order_id
    })))
}

/// Resume a paused recurring order
/// POST /api/v1/trading/recurring-orders/:id/resume
#[utoipa::path(
    post,
    path = "/api/v1/trading/recurring-orders/{id}/resume",
    tag = "trading",
    params(("id" = Uuid, Path, description = "Order ID to resume")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order resumed"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Order not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn resume_recurring_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    info!("Resuming recurring order {} for user {}", order_id, user.0.sub);

    // Get current order to recalculate next execution
    let order = sqlx::query!(
        "SELECT interval_type as \"interval_type!: IntervalType\", interval_value FROM recurring_orders WHERE id = $1 AND user_id = $2",
        order_id,
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to get order: {}", e)))?
    .ok_or_else(|| ApiError::NotFound("Order not found".to_string()))?;

    let next_execution = calculate_next_execution(order.interval_type, order.interval_value.unwrap_or(1));

    let result = sqlx::query!(
        r#"
        UPDATE recurring_orders
        SET status = 'active', next_execution_at = $3, updated_at = NOW()
        WHERE id = $1 AND user_id = $2 AND status = 'paused'
        "#,
        order_id,
        user.0.sub,
        next_execution
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to resume recurring order: {}", e);
        ApiError::Internal(format!("Failed to resume order: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Order not found or not paused".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Recurring order resumed",
        "order_id": order_id,
        "next_execution_at": next_execution
    })))
}
