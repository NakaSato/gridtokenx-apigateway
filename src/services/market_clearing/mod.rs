pub mod types;
pub mod epoch;
pub mod orders;
pub mod matching;
pub mod blockchain;
pub mod escrow;

use sqlx::PgPool;

pub use types::*;

use crate::config::Config;
use crate::services::{AuditLogger, BlockchainService, WalletService, WebSocketService};

#[derive(Clone, Debug)]
pub struct MarketClearingService {
    db: PgPool,
    blockchain_service: BlockchainService,
    config: Config,
    wallet_service: WalletService,
    audit_logger: AuditLogger,
    websocket_service: WebSocketService,
}

impl MarketClearingService {
    pub fn new(
        db: PgPool,
        blockchain_service: BlockchainService,
        config: Config,
        wallet_service: WalletService,
        audit_logger: AuditLogger,
        websocket_service: WebSocketService,
    ) -> Self {
        Self {
            db,
            blockchain_service,
            config,
            wallet_service,
            audit_logger,
            websocket_service,
        }
    }
}
