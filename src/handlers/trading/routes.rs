use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::app_state::AppState;
use super::orders::{create_order, cancel_order, update_order, get_order_book, get_user_orders, get_my_trades, get_token_balance};
use super::blockchain::{get_blockchain_market_data, match_blockchain_orders};
use super::p2p::{calculate_p2p_cost, get_p2p_market_prices};
use super::status::{get_matching_status, get_settlement_stats};
use super::revenue::{get_revenue_summary, get_revenue_records};

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
        
        // Status & Monitoring
        .route("/matching-status", get(get_matching_status))
        .route("/settlement-stats", get(get_settlement_stats))
        
        // Revenue (Admin)
        .route("/revenue/summary", get(get_revenue_summary))
        .route("/revenue/records", get(get_revenue_records))
        
        // Admin
        .route("/admin/match-orders", post(match_blockchain_orders))
}

