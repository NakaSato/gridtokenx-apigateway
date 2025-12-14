// use crate::database::schema::types::OrderSide as DbOrderSide; // Unused
use crate::error::ApiError;
use crate::services::market::order_book::OrderBook;
use crate::services::market::types::{ClearingPrice, OrderSide, TradeMatch};
use chrono::Utc;
use rust_decimal::Decimal;
use tracing::{debug, info};
use uuid::Uuid;

pub struct MatchingEngine;

impl MatchingEngine {
    /// Calculate market clearing price using supply-demand curves
    pub fn calculate_clearing_price(book: &OrderBook) -> Option<ClearingPrice> {
        let buy_depth = book.buy_depth();
        let sell_depth = book.sell_depth();

        if buy_depth.is_empty() || sell_depth.is_empty() {
            return None;
        }

        // Build cumulative supply and demand curves
        let mut demand_curve: Vec<(Decimal, Decimal)> = Vec::new();
        let mut cumulative_demand = Decimal::ZERO;
        for (price, volume) in buy_depth {
            cumulative_demand += volume;
            demand_curve.push((price, cumulative_demand));
        }

        let mut supply_curve: Vec<(Decimal, Decimal)> = Vec::new();
        let mut cumulative_supply = Decimal::ZERO;
        for (price, volume) in sell_depth {
            cumulative_supply += volume;
            supply_curve.push((price, cumulative_supply));
        }

        // Find intersection point (clearing price)
        let mut best_clearing: Option<ClearingPrice> = None;
        let mut max_volume = Decimal::ZERO;

        for (demand_price, demand_vol) in &demand_curve {
            for (supply_price, supply_vol) in &supply_curve {
                // Can only clear if buyers willing to pay >= sellers asking
                if demand_price >= supply_price {
                    let clearable_volume = (*demand_vol).min(*supply_vol);

                    if clearable_volume > max_volume {
                        max_volume = clearable_volume;
                        // Clearing price is midpoint of bid-ask spread
                        let clearing_price = (*demand_price + *supply_price) / Decimal::TWO;

                        best_clearing = Some(ClearingPrice {
                            price: clearing_price,
                            volume: clearable_volume,
                            buy_orders_count: demand_curve.len(),
                            sell_orders_count: supply_curve.len(),
                        });
                    }
                }
            }
        }

        best_clearing
    }

    /// Match orders at market clearing price with atomic partial fill handling
    /// Returns a tuple of (Matches, Expired Order IDs)
    pub fn match_orders(book: &mut OrderBook) -> Result<(Vec<TradeMatch>, Vec<Uuid>), ApiError> {
        let mut matches = Vec::new();

        // Remove expired orders first
        let expired = book.remove_expired_orders();
        if !expired.is_empty() {
            debug!("🗑️  Removed {} expired orders from memory", expired.len());
        }

        // Continuous matching loop until no more crosses exist
        loop {
            let best_bid = book.best_bid();
            let best_ask = book.best_ask();

            match (best_bid, best_ask) {
                (Some(bid), Some(ask)) if bid >= ask => {
                    // There's overlap - we can match orders
                    debug!("🔄 Market crossover: Bid ${} >= Ask ${}", bid, ask);

                    // Get the best sell order (lowest ask)
                    let sell_order = {
                        let (_, sell_level) = book
                            .sell_levels
                            .iter_mut()
                            .next()
                            .ok_or(ApiError::Internal("No sell orders available".into()))?;

                        if sell_level.orders.is_empty() {
                            break; // No more sell orders
                        }

                        sell_level.orders[0].clone()
                    };

                    if sell_order.is_filled() || sell_order.is_expired() {
                        book.remove_order(&sell_order.id);
                        continue;
                    }

                    // Get the best buy order (highest bid)
                    let buy_order = {
                        let (_, buy_level) = book
                            .buy_levels
                            .iter_mut()
                            .rev()
                            .next()
                            .ok_or(ApiError::Internal("No buy orders available".into()))?;

                        if buy_level.orders.is_empty() {
                            break; // No more buy orders
                        }

                        buy_level.orders[0].clone()
                    };

                    if buy_order.is_filled() || buy_order.is_expired() {
                        book.remove_order(&buy_order.id);
                        continue;
                    }

                    // Verify orders can still match
                    if buy_order.price < sell_order.price {
                        break; // No more matches possible
                    }

                    // Calculate match quantity (minimum of remaining amounts)
                    let sell_remaining = sell_order.remaining_amount();
                    let buy_remaining = buy_order.remaining_amount();
                    let match_quantity = sell_remaining.min(buy_remaining);

                    if match_quantity <= Decimal::ZERO {
                        break;
                    }

                    // Execution price is midpoint of bid-ask spread
                    let execution_price = (buy_order.price + sell_order.price) / Decimal::TWO;
                    let total_value = match_quantity * execution_price;

                    // Create trade match
                    let trade = TradeMatch {
                        buy_order_id: buy_order.id,
                        sell_order_id: sell_order.id,
                        buyer_id: buy_order.user_id,
                        seller_id: sell_order.user_id,
                        price: execution_price,
                        quantity: match_quantity,
                        total_value,
                        matched_at: Utc::now(),
                        epoch_id: buy_order
                            .epoch_id
                            .or(sell_order.epoch_id)
                            .unwrap_or_else(Uuid::new_v4),
                    };

                    info!(
                        "✅ Matched: {} kWh at ${}/kWh (buyer: {}, seller: {})",
                        match_quantity, execution_price, buy_order.user_id, sell_order.user_id
                    );

                    // Update order filled amounts in-memory (atomic update)
                    Self::update_order_filled_amount_in_book(book, &buy_order.id, match_quantity)?;
                    Self::update_order_filled_amount_in_book(book, &sell_order.id, match_quantity)?;

                    // Remove fully filled orders from book
                    if buy_order.remaining_amount() + match_quantity >= buy_order.energy_amount {
                        book.remove_order(&buy_order.id);
                        debug!("Removed fully filled buy order: {}", buy_order.id);
                    }
                    if sell_order.remaining_amount() + match_quantity >= sell_order.energy_amount {
                        book.remove_order(&sell_order.id);
                        debug!("Removed fully filled sell order: {}", sell_order.id);
                    }

                    matches.push(trade);
                }
                (Some(bid), Some(ask)) => {
                    debug!("No market crossover: Bid ${} < Ask ${}", bid, ask);
                    break;
                }
                _ => {
                    debug!("Insufficient market depth for matching");
                    break;
                }
            }
        }

        Ok((matches, expired))
    }

    /// Update order filled amount in the order book (in-memory only)
    fn update_order_filled_amount_in_book(
        book: &mut OrderBook,
        order_id: &Uuid,
        amount: Decimal,
    ) -> Result<(), ApiError> {
        // Find the order in the book and update its filled amount
        let side = book
            .order_index
            .get(order_id)
            .ok_or(ApiError::NotFound("Order not found in book".into()))?;

        let levels = match side {
            OrderSide::Buy => &mut book.buy_levels,
            OrderSide::Sell => &mut book.sell_levels,
        };

        for (_, level) in levels.iter_mut() {
            for order in level.orders.iter_mut() {
                if &order.id == order_id {
                    order.filled_amount += amount;
                    level.total_volume -= amount;
                    return Ok(());
                }
            }
        }

        Err(ApiError::NotFound("Order not found in price level".into()))
    }
}
