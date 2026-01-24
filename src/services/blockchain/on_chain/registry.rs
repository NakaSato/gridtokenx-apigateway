use anyhow::Result;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use crate::services::blockchain::instructions::InstructionBuilder;
use crate::services::blockchain::transactions::TransactionHandler;
use tracing::info;

#[derive(Clone, Debug)]
pub struct RegistryManager {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
}

impl RegistryManager {
    pub fn new(transaction_handler: TransactionHandler, instruction_builder: InstructionBuilder) -> Self {
        Self {
            transaction_handler,
            instruction_builder,
        }
    }

    pub async fn initialize_registry(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Registry on-chain...");
        let instruction = self.instruction_builder.build_initialize_registry_instruction()?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn initialize_oracle(&self, authority: &Keypair, api_gateway: &Pubkey) -> Result<Signature> {
        info!("Initializing Oracle on-chain with API Gateway: {}...", api_gateway);
        let instruction = self.instruction_builder.build_initialize_oracle_instruction(api_gateway)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn update_price(
        &self,
        authority: &Keypair,
        price_feed_id: &str,
        price: u64,
        confidence: u64,
    ) -> Result<Signature> {
        info!("Updating Oracle price for feed {}: {}...", price_feed_id, price);
        let instruction = self.instruction_builder.build_update_price_instruction(price_feed_id, price, confidence)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn update_meter_reading(
        &self,
        authority: &Keypair,
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Signature> {
        info!("Updating meter reading on-chain for {}...", meter_id);
        let instruction = self.instruction_builder.build_update_meter_reading_instruction(
            meter_id, produced, consumed, timestamp
        )?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn register_meter(
        &self,
        authority: &Keypair,
        meter_id: &str,
        meter_type: u8,
    ) -> Result<Signature> {
        info!("Registering meter {} on-chain...", meter_id);
        let instruction = self.instruction_builder.build_register_meter_instruction(meter_id, meter_type)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn register_user_on_chain(
        &self,
        user_authority: &Keypair,
        user_type: u8,
        location: &str,
    ) -> Result<Signature> {
        info!("Registering user {} on-chain...", user_authority.pubkey());
        let registry_pda = Pubkey::from_str(&std::env::var("SOLANA_REGISTRY_PDA")?)?;
        let instruction = self.instruction_builder.build_register_user_instruction(
            &user_authority.pubkey(), &registry_pda, user_type, location
        )?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&user_authority.pubkey()))).await
    }
}
