use axum::{extract::State, response::Json};

use super::types::{TradeHistory, TradeRecord};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::AppState;

/// Get user's trade history
#[utoipa::path(
    get,
    path = "/api/market/trades/my-history",
    responses(
        (status = 200, description = "Trade history retrieved", body = TradeHistory),
        (status = 500, description = "Internal server error")
    ),
    tag = "Market Data",
    security(("bearer_auth" = []))
)]
pub async fn get_my_trade_history(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<TradeHistory>, ApiError> {
    let user_id = user.0.sub;

    // Query order_matches table and join with trading_orders to get user information
    let trades = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        r#"
        SELECT 
            om.id::text,
            om.buy_order_id::text,
            om.sell_order_id::text,
            buy_order.user_id::text as buyer_id,
            sell_order.user_id::text as seller_id,
            om.matched_amount::text as quantity,
            om.match_price::text as price,
            (om.matched_amount * om.match_price)::text as total_value,
            om.match_time::text as executed_at,
            om.status,
            CASE 
                WHEN buy_order.user_id = $1 THEN 'buyer'
                ELSE 'seller'
            END as role
        FROM order_matches om
        INNER JOIN trading_orders buy_order ON om.buy_order_id = buy_order.id
        INNER JOIN trading_orders sell_order ON om.sell_order_id = sell_order.id
        WHERE buy_order.user_id = $1 OR sell_order.user_id = $1
        ORDER BY om.match_time DESC
        LIMIT 50
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let total_count = trades.len() as i64;

    let trade_records: Vec<TradeRecord> = trades
        .into_iter()
        .map(
            |(
                id,
                buy_order_id,
                sell_order_id,
                buyer_id,
                seller_id,
                quantity,
                price,
                total_value,
                executed_at,
                status,
                role,
            )| {
                let counterparty_id = if role == "buyer" { seller_id } else { buyer_id };

                TradeRecord {
                    id,
                    buy_order_id,
                    sell_order_id,
                    quantity,
                    price,
                    total_value,
                    role,
                    counterparty_id,
                    executed_at,
                    status,
                }
            },
        )
        .collect();

    Ok(Json(TradeHistory {
        trades: trade_records,
        total_count,
    }))
}
