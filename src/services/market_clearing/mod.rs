pub mod types;
pub mod epoch;
pub mod orders;
pub mod matching;
pub mod blockchain;
pub mod escrow;
pub mod revenue;

use sqlx::PgPool;
use rust_decimal::Decimal;

pub use types::*;

use crate::config::Config;
use crate::services::{AuditLogger, BlockchainService, WalletService, WebSocketService, ErcService};

#[derive(Clone, Debug)]
pub struct MarketClearingService {
    db: PgPool,
    blockchain_service: BlockchainService,
    config: Config,
    wallet_service: WalletService,
    audit_logger: AuditLogger,
    websocket_service: WebSocketService,
    erc_service: ErcService,
}

impl MarketClearingService {
    pub fn new(
        db: PgPool,
        blockchain_service: BlockchainService,
        config: Config,
        wallet_service: WalletService,
        audit_logger: AuditLogger,
        websocket_service: WebSocketService,
        erc_service: ErcService,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            config,
            wallet_service,
            audit_logger,
            websocket_service,
            erc_service,
        }
    }

    /// Calculate market clearing price from order book
    /// Uses midpoint of bid-ask spread where supply meets demand
    pub fn calculate_clearing_price(
        buy_orders: &[OrderBookEntry],
        sell_orders: &[OrderBookEntry],
    ) -> Option<ClearingPrice> {
        if buy_orders.is_empty() || sell_orders.is_empty() {
            return None;
        }

        // Get best bid (highest buy price) and best ask (lowest sell price)
        let best_bid = buy_orders.iter()
            .map(|o| o.price_per_kwh)
            .max()?;
        let best_ask = sell_orders.iter()
            .map(|o| o.price_per_kwh)
            .min()?;

        // No clearing price if bid < ask (no overlap)
        if best_bid < best_ask {
            return None;
        }

        // Calculate clearing price as midpoint
        let clearing_price = (best_bid + best_ask) / Decimal::from(2);

        // Calculate clearable volume (sum of orders that can trade)
        let buy_volume: Decimal = buy_orders.iter()
            .filter(|o| o.price_per_kwh >= best_ask)
            .map(|o| o.energy_amount)
            .sum();
        let sell_volume: Decimal = sell_orders.iter()
            .filter(|o| o.price_per_kwh <= best_bid)
            .map(|o| o.energy_amount)
            .sum();
        let clearable_volume = buy_volume.min(sell_volume);

        Some(ClearingPrice {
            price: clearing_price,
            volume: clearable_volume,
            buy_orders_count: buy_orders.len(),
            sell_orders_count: sell_orders.len(),
            best_bid,
            best_ask,
        })
    }
}
