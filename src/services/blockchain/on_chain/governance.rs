use anyhow::Result;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use crate::services::blockchain::instructions::InstructionBuilder;
use crate::services::blockchain::transactions::TransactionHandler;
use tracing::info;

#[derive(Clone, Debug)]
pub struct GovernanceManager {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
}

impl GovernanceManager {
    pub fn new(transaction_handler: TransactionHandler, instruction_builder: InstructionBuilder) -> Self {
        Self {
            transaction_handler,
            instruction_builder,
        }
    }

    pub async fn initialize_governance(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Governance on-chain...");
        let instruction = self.instruction_builder.build_initialize_governance_instruction()?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn issue_erc(
        &self,
        authority: &Keypair,
        certificate_id: &str,
        user_wallet: &Pubkey,
        meter_account: &Pubkey,
        energy_amount: u64,
        renewable_source: &str,
        validation_data: &str,
    ) -> Result<Signature> {
        info!("Issuing ERC {} on-chain...", certificate_id);
        let instruction = self.instruction_builder.build_issue_erc_instruction(
            certificate_id, user_wallet, meter_account, energy_amount, renewable_source, validation_data
        )?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&authority.pubkey()))).await
    }

    pub async fn transfer_erc(
        &self,
        owner: &Keypair,
        certificate_id: &str,
        new_owner: &Pubkey,
    ) -> Result<Signature> {
        info!("Transferring ERC {} on-chain...", certificate_id);
        let instruction = self.instruction_builder.build_transfer_erc_instruction(certificate_id, &owner.pubkey(), new_owner)?;
        self.transaction_handler.submit_transaction(solana_sdk::transaction::Transaction::new_with_payer(&[instruction], Some(&owner.pubkey()))).await
    }
}
