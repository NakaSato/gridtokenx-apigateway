//! Order book implementation for market clearing

use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use super::types::{BookOrder, OrderSide};

/// Price level in the order book
#[derive(Debug, Clone)]
pub(super) struct PriceLevel {
    pub price: Decimal,
    pub total_volume: Decimal,
    pub orders: Vec<BookOrder>,
}

impl PriceLevel {
    pub fn new(price: Decimal) -> Self {
        Self {
            price,
            total_volume: Decimal::ZERO,
            orders: Vec::new(),
        }
    }

    pub fn add_order(&mut self, order: BookOrder) {
        self.total_volume += order.remaining_amount();
        self.orders.push(order);
    }

    pub fn remove_order(&mut self, order_id: &Uuid) -> Option<BookOrder> {
        if let Some(pos) = self.orders.iter().position(|o| &o.id == order_id) {
            let order = self.orders.remove(pos);
            self.total_volume -= order.remaining_amount();
            Some(order)
        } else {
            None
        }
    }
}

/// Order book with buy and sell sides
#[derive(Debug, Clone)]
pub struct OrderBook {
    // Buy orders sorted by price (descending) - highest bids first
    pub(super) buy_levels: BTreeMap<String, PriceLevel>, // String key for decimal sorting
    // Sell orders sorted by price (ascending) - lowest asks first
    pub(super) sell_levels: BTreeMap<String, PriceLevel>,
    // Quick lookup by order ID
    pub(super) order_index: HashMap<Uuid, OrderSide>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            buy_levels: BTreeMap::new(),
            sell_levels: BTreeMap::new(),
            order_index: HashMap::new(),
        }
    }

    /// Add order to the book
    pub fn add_order(&mut self, order: BookOrder) {
        let price_key = Self::price_key(order.price);

        match order.side {
            OrderSide::Buy => {
                let level = self
                    .buy_levels
                    .entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                level.add_order(order.clone());
            }
            OrderSide::Sell => {
                let level = self
                    .sell_levels
                    .entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                level.add_order(order.clone());
            }
        }

        self.order_index.insert(order.id, order.side);
    }

    /// Remove order from the book
    pub fn remove_order(&mut self, order_id: &Uuid) -> Option<BookOrder> {
        let side = self.order_index.remove(order_id)?;

        let mut order_removed = None;

        match side {
            OrderSide::Buy => {
                // Search through buy levels
                let mut empty_price_key = None;
                for (price_key, level) in self.buy_levels.iter_mut() {
                    if let Some(order) = level.remove_order(order_id) {
                        order_removed = Some(order);
                        if level.orders.is_empty() {
                            empty_price_key = Some(price_key.clone());
                        }
                        break;
                    }
                }
                if let Some(key) = empty_price_key {
                    self.buy_levels.remove(&key);
                }
            }
            OrderSide::Sell => {
                // Search through sell levels
                let mut empty_price_key = None;
                for (price_key, level) in self.sell_levels.iter_mut() {
                    if let Some(order) = level.remove_order(order_id) {
                        order_removed = Some(order);
                        if level.orders.is_empty() {
                            empty_price_key = Some(price_key.clone());
                        }
                        break;
                    }
                }
                if let Some(key) = empty_price_key {
                    self.sell_levels.remove(&key);
                }
            }
        }

        order_removed
    }

    /// Get best bid (highest buy price)
    pub fn best_bid(&self) -> Option<Decimal> {
        self.buy_levels
            .iter()
            .next_back()
            .map(|(_, level)| level.price)
    }

    /// Get best ask (lowest sell price)
    pub fn best_ask(&self) -> Option<Decimal> {
        self.sell_levels.iter().next().map(|(_, level)| level.price)
    }

    /// Calculate mid-market price
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Get spread (difference between best ask and best bid)
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get total volume at each price level for buy side
    pub fn buy_depth(&self) -> Vec<(Decimal, Decimal)> {
        self.buy_levels
            .iter()
            .rev() // Highest prices first
            .map(|(_, level)| (level.price, level.total_volume))
            .collect()
    }

    /// Get total volume at each price level for sell side
    pub fn sell_depth(&self) -> Vec<(Decimal, Decimal)> {
        self.sell_levels
            .iter()
            .map(|(_, level)| (level.price, level.total_volume))
            .collect()
    }

    /// Convert price to sortable string key
    fn price_key(price: Decimal) -> String {
        // Pad with zeros for proper string sorting
        format!("{:020.8}", price)
    }

    /// Clear expired orders
    pub fn remove_expired_orders(&mut self) -> Vec<Uuid> {
        let mut expired_ids = Vec::new();

        // Find expired buy orders
        for level in self.buy_levels.values_mut() {
            let before_count = level.orders.len();
            level.orders.retain(|order| {
                if order.is_expired() {
                    expired_ids.push(order.id);
                    false
                } else {
                    true
                }
            });
            if level.orders.len() < before_count {
                level.total_volume = level.orders.iter().map(|o| o.remaining_amount()).sum();
            }
        }

        // Find expired sell orders
        for level in self.sell_levels.values_mut() {
            let before_count = level.orders.len();
            level.orders.retain(|order| {
                if order.is_expired() {
                    expired_ids.push(order.id);
                    false
                } else {
                    true
                }
            });
            if level.orders.len() < before_count {
                level.total_volume = level.orders.iter().map(|o| o.remaining_amount()).sum();
            }
        }

        // Clean up empty levels
        self.buy_levels.retain(|_, level| !level.orders.is_empty());
        self.sell_levels.retain(|_, level| !level.orders.is_empty());

        // Update index
        for id in &expired_ids {
            self.order_index.remove(id);
        }

        expired_ids
    }

    /// Clear all orders from the book
    pub fn clear(&mut self) {
        self.buy_levels.clear();
        self.sell_levels.clear();
        self.order_index.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_order_book_creation() {
        let book = OrderBook::new();
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
    }

    #[test]
    fn test_add_buy_order() {
        let mut book = OrderBook::new();
        let order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert_eq!(
            book.best_bid(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_price_priority() {
        let mut book = OrderBook::new();

        // Add buy orders at different prices
        let order1 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let order2 = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.20").unwrap(), // Higher price
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order1);
        book.add_order(order2);

        // Best bid should be the highest price
        assert_eq!(
            book.best_bid(),
            Some(Decimal::from_str_exact("0.20").unwrap())
        );
    }

    #[test]
    fn test_add_sell_order() {
        let mut book = OrderBook::new();
        let order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert_eq!(
            book.best_ask(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }

    #[test]
    fn test_order_removal() {
        let mut book = OrderBook::new();
        let order_id = Uuid::new_v4();
        let order = BookOrder {
            id: order_id,
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.15").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(order);
        assert!(book.best_bid().is_some());

        book.remove_order(&order_id);
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_mid_price_calculation() {
        let mut book = OrderBook::new();

        let buy_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Buy,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.10").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        let sell_order = BookOrder {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            side: OrderSide::Sell,
            energy_amount: Decimal::from(100),
            price: Decimal::from_str_exact("0.20").unwrap(),
            filled_amount: Decimal::ZERO,
            created_at: Utc::now(),
            epoch_id: None,
            expires_at: Utc::now() + chrono::Duration::hours(24),
        };

        book.add_order(buy_order);
        book.add_order(sell_order);

        // Mid price should be (0.10 + 0.20) / 2 = 0.15
        assert_eq!(
            book.mid_price(),
            Some(Decimal::from_str_exact("0.15").unwrap())
        );
    }
}
