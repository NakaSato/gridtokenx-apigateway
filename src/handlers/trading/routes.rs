use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::app_state::AppState;
use super::orders::{create_order, cancel_order, update_order, get_order_book, get_user_orders, get_my_trades, get_token_balance};
use super::blockchain::{get_blockchain_market_data, match_blockchain_orders};
use super::p2p::{calculate_p2p_cost, get_p2p_market_prices};

/// Build the v1 trading routes
pub fn v1_trading_routes() -> Router<AppState> {
    Router::new()
        // Orders
        .route("/orders", post(create_order).get(get_user_orders))
        .route("/orders/{id}", delete(cancel_order).put(update_order))
        
        // Order Book
        .route("/orderbook", get(get_order_book))
        
        // Trade History
        .route("/trades", get(get_my_trades))
        
        // Token Balance
        .route("/balance", get(get_token_balance))
        
        // Market Data
        .route("/market/blockchain", get(get_blockchain_market_data))
        
        // P2P Transaction Cost & Pricing
        .route("/p2p/calculate-cost", post(calculate_p2p_cost))
        .route("/p2p/market-prices", get(get_p2p_market_prices))
        
        // Admin
        .route("/admin/match-orders", post(match_blockchain_orders))
}
