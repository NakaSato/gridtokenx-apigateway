//! Market Clearing Engine for P2P Energy Trading
//!
//! This module implements the market clearing engine with price discovery
//! and order matching for peer-to-peer energy trading.

pub mod clearing;
pub mod order_book;
pub mod types;

// Re-export main types for convenience
pub use clearing::MarketClearingEngine;
pub use order_book::OrderBook;
pub use types::{BookOrder, ClearingPrice, OrderBookSnapshot, OrderSide, TradeMatch};
