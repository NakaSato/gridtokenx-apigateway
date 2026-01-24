pub mod registry;
pub mod trading;
pub mod tokens;
pub mod governance;

pub use registry::RegistryManager;
pub use trading::TradingManager;
pub use tokens::TokenManager;
pub use governance::GovernanceManager;

use anyhow::{anyhow, Result};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature};
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use crate::config::SolanaProgramsConfig;
use crate::services::blockchain::instructions::InstructionBuilder;
use crate::services::blockchain::transactions::TransactionHandler;

/// Manages On-Chain transactions and program interactions - Refactored
#[derive(Clone, Debug)]
pub struct OnChainManager {
    pub registry: RegistryManager,
    pub trading: TradingManager,
    pub tokens: TokenManager,
    pub governance: GovernanceManager,
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
    program_ids: SolanaProgramsConfig,
}

impl OnChainManager {
    pub fn new(
        transaction_handler: TransactionHandler,
        instruction_builder: InstructionBuilder,
        program_ids: SolanaProgramsConfig,
    ) -> Self {
        Self {
            registry: RegistryManager::new(transaction_handler.clone(), instruction_builder.clone()),
            trading: TradingManager::new(transaction_handler.clone(), instruction_builder.clone()),
            tokens: TokenManager::new(transaction_handler.clone(), instruction_builder.clone()),
            governance: GovernanceManager::new(transaction_handler.clone(), instruction_builder.clone()),
            transaction_handler,
            instruction_builder,
            program_ids,
        }
    }

    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<Signature> {
        self.transaction_handler.submit_transaction(transaction).await
    }

    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        self.transaction_handler.confirm_transaction(signature).await
    }

    pub fn trading_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.trading_program_id)
            .map_err(|e| anyhow!("Invalid Trading ID: {}", e))
    }

    pub fn instruction_builder(&self) -> &InstructionBuilder {
        &self.instruction_builder
    }
}
