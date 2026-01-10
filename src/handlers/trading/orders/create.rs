use axum::{extract::State, response::Json};
use chrono::Utc;


use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::OrderStatus;
use crate::error::{ApiError, Result};
use crate::models::trading::CreateOrderRequest;
use crate::AppState;
use crate::handlers::websocket::broadcaster::broadcast_p2p_order_update;

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

    // Verify signature if provided (P2P orders)
    if let (Some(signature), Some(timestamp)) = (&payload.signature, payload.timestamp) {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        use hex;

        // Verify timestamp is within 5 minutes window
        let now_ts = Utc::now().timestamp_millis();
        if (now_ts - timestamp).abs() > 5 * 60 * 1000 {
            return Err(ApiError::BadRequest("Order timestamp expired".to_string()));
        }

        // Reconstruct message: side + amount + price + timestamp
        let amount_str = payload.energy_amount.to_string();
        // Handle Option<Decimal> for price
        let price_str = payload.price_per_kwh.map(|p| p.to_string()).unwrap_or_default();
        
        let message = format!("{}:{}:{}:{}", 
            payload.side,
            amount_str,
            price_str,
            timestamp
        );

        // TODO: Get secret key from user wallet/config. For now using placeholder.
        let secret_key = "test_secret_key";
        
        let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
            .map_err(|e| ApiError::Internal(format!("HMAC init failed: {}", e)))?;
        
        mac.update(message.as_bytes());
        let result = mac.finalize();
        let expected_signature = hex::encode(result.into_bytes());

        if signature != &expected_signature {
           tracing::warn!("Invalid signature. Expected: {}, Got: {}", expected_signature, signature);
           return Err(ApiError::BadRequest("Invalid order signature".to_string()));
        }
        
        tracing::info!("P2P Order signature verified successfully");
    }

    // Auto-detect zone if not provided
    let zone_id = if let Some(zid) = payload.zone_id {
        Some(zid)
    } else {
        // Try to find user's zone from their registered meter
        let meter_zone = sqlx::query!(
            "SELECT zone_id FROM meter_registry WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
            user.0.sub
        )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
        .and_then(|r| r.zone_id);
        
        if meter_zone.is_none() {
            tracing::warn!("User {} has no registered meter/zone. Defaulting to unknown zone.", user.0.sub);
        }
        meter_zone
    };

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
            zone_id,
            payload.meter_id,
            payload.session_token.as_deref(),
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

    // Broadcast P2P order creation via WebSocket
    if let Err(e) = broadcast_p2p_order_update(
        order_id,
        user.0.sub,
        payload.side.to_string(),
        "open".to_string(),
        payload.energy_amount.to_string(),
        "0".to_string(), // filled_amount
        payload.energy_amount.to_string(), // remaining_amount
        payload.price_per_kwh.map(|p| p.to_string()).unwrap_or_default(),
    ).await {
        tracing::warn!("Failed to broadcast order creation: {}", e);
    }

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
