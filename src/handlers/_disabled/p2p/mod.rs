use axum::{extract::State, Json};
use crate::error::{ApiError, Result};
use crate::auth::middleware::AuthenticatedUser;
use crate::services::p2p::CreateOrderRequest;
use crate::AppState;
use crate::handlers::ApiResponse;

pub async fn create_order(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<CreateOrderRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let order_id = state.p2p_service.create_order(user.0, req).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "id": order_id }))))
}

pub async fn get_order_book(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<crate::services::p2p::OrderBookResponse>>, ApiError> {
    let order_book = state.p2p_service.get_order_book().await?;
    Ok(Json(ApiResponse::success(order_book)))
}

pub async fn get_my_orders(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::services::p2p::P2POrder>>>, ApiError> {
    let orders = state.p2p_service.get_user_orders(user.0).await?;
    Ok(Json(ApiResponse::success(orders)))
}
