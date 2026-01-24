use anyhow::Result;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use crate::services::blockchain::instructions::InstructionBuilder;
use crate::services::blockchain::transactions::TransactionHandler;
use tracing::info;

#[derive(Clone, Debug)]
pub struct TokenManager {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
}

impl TokenManager {
    pub fn new(transaction_handler: TransactionHandler, instruction_builder: InstructionBuilder) -> Self {
        Self {
            transaction_handler,
            instruction_builder,
        }
    }

    pub async fn initialize_token(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Energy Token on-chain...");
        let instruction = self.instruction_builder.build_initialize_energy_token_instruction(authority.pubkey())?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn mint_tokens(&self, authority: &Keypair, recipient: &str, amount: u64) -> Result<Signature> {
        info!("Minting {} tokens to {}...", amount, recipient);
        let instruction = self.instruction_builder.build_mint_instruction(recipient, amount)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn transfer_tokens(
        &self,
        authority: &Keypair,
        from: &str,
        to: &str,
        amount: u64,
        mint: &str,
    ) -> Result<Signature> {
        info!("Transferring {} tokens from {} to {}...", amount, from, to);
        let instruction = self.instruction_builder.build_transfer_instruction(from, to, amount, mint)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }
}
