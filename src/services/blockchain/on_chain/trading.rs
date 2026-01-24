use anyhow::Result;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use crate::services::blockchain::instructions::InstructionBuilder;
use crate::services::blockchain::transactions::TransactionHandler;
use tracing::info;

#[derive(Clone, Debug)]
pub struct TradingManager {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
}

impl TradingManager {
    pub fn new(transaction_handler: TransactionHandler, instruction_builder: InstructionBuilder) -> Self {
        Self {
            transaction_handler,
            instruction_builder,
        }
    }

    pub async fn initialize_market(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Trading Market on-chain...");
        let instruction = self.instruction_builder.build_initialize_market_instruction(authority.pubkey())?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn execute_settlement(
        &self,
        market_authority: &Keypair,
        settlement_ix: solana_sdk::instruction::Instruction,
    ) -> Result<Signature> {
        info!("Executing atomic settlement on-chain...");
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[settlement_ix], Some(&market_authority.pubkey()))).await
    }

    pub async fn swap_energy(
        &self,
        user: &Keypair,
        swap_ix: solana_sdk::instruction::Instruction,
    ) -> Result<Signature> {
        info!("Executing AMM swap on-chain...");
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[swap_ix], Some(&user.pubkey()))).await
    }

    pub async fn derive_order_pda(
        &self,
        authority: &Pubkey,
        market_address: &Pubkey,
    ) -> Result<Pubkey> {
        let account = self.transaction_handler.get_account(market_address).await?;
        if account.data.len() < 44 {
            return Err(anyhow::anyhow!("Market account data too small"));
        }
        let active_orders_bytes: [u8; 4] = account.data[40..44].try_into().unwrap();
        let active_orders = u32::from_le_bytes(active_orders_bytes);

        let trading_program_id = Pubkey::from_str(&std::env::var("SOLANA_TRADING_PROGRAM_ID")?)?;
        let (order_pda, _) = Pubkey::find_program_address(
            &[
                b"order",
                authority.as_ref(),
                market_address.as_ref(),
                &active_orders.to_le_bytes(),
            ],
            &trading_program_id,
        );
        Ok(order_pda)
    }

    pub async fn execute_create_order(
        &self,
        authority: &Keypair,
        market_address: &Pubkey,
        side: u8,
        quantity: u64,
        price: u64,
    ) -> Result<(Signature, String)> {
        info!("Creating order on-chain for market {}...", market_address);
        let order_pda = self.derive_order_pda(&authority.pubkey(), market_address).await?;
        let instruction = self.instruction_builder.build_create_order_instruction(
            market_address, &order_pda, side, quantity, price
        )?;
        let sig = self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await?;
        Ok((sig, order_pda.to_string()))
    }
}
