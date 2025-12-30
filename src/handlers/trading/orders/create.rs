use axum::{extract::State, response::Json};
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};

use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::error::{ApiError, Result};
use crate::models::trading::CreateOrderRequest;
use crate::services::AuditEvent;
use crate::AppState;

use crate::handlers::trading::types::CreateOrderResponse;

/// Create a new trading order
/// POST /api/trading/orders
#[utoipa::path(
    post,
    path = "/api/trading/orders",
    tag = "trading",
    request_body = CreateOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order created successfully", body = CreateOrderResponse),
        (status = 400, description = "Invalid order parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>> {
    tracing::info!("Creating trading order for user: {}", user.0.sub);

    // Call MarketClearingService to handle order creation (DB + On-Chain)
    let order_id = state
        .market_clearing
        .create_order(
            user.0.sub,
            payload.side,
            payload.order_type,
            payload.energy_amount,
            payload.price_per_kwh,
            payload.expiry_time,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create order via service: {}", e);
            ApiError::Internal(format!("Order creation failed: {}", e))
        })?;

    // Get epoch info for response message
    let now = Utc::now();
    let epoch = state.market_clearing.get_or_create_epoch(now).await.map_err(|e| {
        tracing::error!("Failed to get epoch: {}", e);
        ApiError::Internal("Failed to assign order to epoch".to_string())
    })?;

    Ok(Json(CreateOrderResponse {
        id: order_id,
        status: OrderStatus::Pending,
        created_at: now,
        message: format!(
            "Order created successfully and assigned to epoch {} for matching.",
            epoch.epoch_number
        ),
    }))
}
