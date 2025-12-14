use axum::{extract::{State, Path, Query}, Json};
use uuid::Uuid;
use crate::AppState;
use crate::error::ApiError;
use crate::services::futures::{FuturesProduct, FuturesPosition};
use serde::Deserialize;
use rust_decimal::Decimal;
use crate::handlers::ApiResponse;
use crate::auth::middleware::AuthenticatedUser;

#[derive(Deserialize)]
pub struct CreateFuturesOrderRequest {
    pub product_id: Uuid,
    pub side: String, // 'long' or 'short'
    pub order_type: String, // 'market' or 'limit'
    pub quantity: Decimal,
    pub price: Decimal,
    pub leverage: i32,
}

pub async fn get_products(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<FuturesProduct>>>, ApiError> {
    let products = state.futures_service.get_products().await?;
    Ok(Json(ApiResponse::success(products)))
}

pub async fn create_order(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<CreateFuturesOrderRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let order_id = state.futures_service.create_order(
        user.0,
        req.product_id,
        req.side,
        req.order_type,
        req.quantity,
        req.price,
        req.leverage,
    ).await?;

    Ok(Json(ApiResponse::success(serde_json::json!({ "order_id": order_id }))))
}

pub async fn get_positions(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<FuturesPosition>>>, ApiError> {
    let positions = state.futures_service.get_positions(user.0).await?;
    Ok(Json(ApiResponse::success(positions)))
}

#[derive(Deserialize)]
pub struct GetCandlesRequest {
    pub product_id: Uuid,
    pub interval: String,
}

pub async fn get_candles(
    State(state): State<AppState>,
    Query(req): Query<GetCandlesRequest>,
) -> Result<Json<ApiResponse<Vec<crate::services::futures::Candle>>>, ApiError> {
    let candles = state.futures_service.get_candles(req.product_id, req.interval).await?;
    Ok(Json(ApiResponse::success(candles)))
}

#[derive(Deserialize)]
pub struct GetOrderBookRequest {
    pub product_id: Uuid,
}

pub async fn get_order_book(
    State(state): State<AppState>,
    Query(req): Query<GetOrderBookRequest>,
) -> Result<Json<ApiResponse<crate::services::futures::OrderBook>>, ApiError> {
    let order_book = state.futures_service.get_order_book(req.product_id).await?;
    Ok(Json(ApiResponse::success(order_book)))
}

pub async fn get_my_orders(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::services::futures::FuturesOrder>>>, ApiError> {
    let orders = state.futures_service.get_user_orders(user.0).await?;
    Ok(Json(ApiResponse::success(orders)))
}

pub async fn close_position(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(position_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let order_id = state.futures_service.close_position(user.0, position_id).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "order_id": order_id }))))
}
