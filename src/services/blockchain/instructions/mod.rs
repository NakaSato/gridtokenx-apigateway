pub mod constants;
pub mod registry;
pub mod trading;
pub mod governance;
pub mod tokens;

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub use constants::*;
pub use registry::RegistryInstructions;
pub use trading::TradingInstructions;
pub use governance::GovernanceInstructions;
pub use tokens::TokenInstructions;

/// Instruction builder for Solana programs - Refactored as a thin wrapper
#[derive(Clone, Debug)]
pub struct InstructionBuilder {
    payer: Pubkey,
}

impl InstructionBuilder {
    pub fn new(payer: Pubkey) -> Self {
        Self { payer }
    }

    pub fn payer(&self) -> Pubkey {
        self.payer
    }

    // --- Trading ---

    pub fn build_create_order_instruction(
        &self,
        market_pubkey: &Pubkey,
        _authority: &Pubkey,
        order_pda: Pubkey,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
        payer: Pubkey,
    ) -> Result<Instruction> {
        let erc_pda = if let Some(id) = erc_certificate_id {
            Some(GovernanceInstructions::get_erc_certificate_pubkey(id)?)
        } else {
            None
        };
        TradingInstructions::build_create_order_instruction(
            market_pubkey,
            order_pda,
            energy_amount,
            price_per_kwh,
            order_type,
            erc_pda,
            payer,
        )
    }

    pub fn build_match_orders_instruction(
        &self,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
        trade_record_pubkey: Pubkey,
    ) -> Result<Instruction> {
        TradingInstructions::build_match_orders_instruction(
            self.payer,
            market_pubkey,
            buy_order_pubkey,
            sell_order_pubkey,
            match_amount,
            trade_record_pubkey,
        )
    }

    pub fn build_initialize_market_instruction(&self, authority: Pubkey) -> Result<Instruction> {
        TradingInstructions::build_initialize_market_instruction(authority)
    }

    pub fn build_execute_atomic_settlement_instruction(
        &self,
        market: Pubkey,
        buy_order: Pubkey,
        sell_order: Pubkey,
        buyer_currency_escrow: Pubkey,
        seller_energy_escrow: Pubkey,
        seller_currency_account: Pubkey,
        buyer_energy_account: Pubkey,
        fee_collector: Pubkey,
        wheeling_collector: Pubkey,
        energy_mint: Pubkey,
        currency_mint: Pubkey,
        escrow_authority: Pubkey,
        market_authority: Pubkey,
        amount: u64,
        price: u64,
        wheeling_charge: u64,
        token_program_id: Pubkey,
        secondary_token_program_id: Pubkey,
    ) -> Result<Instruction> {
        TradingInstructions::build_execute_atomic_settlement_instruction(
            market, buy_order, sell_order, buyer_currency_escrow, seller_energy_escrow,
            seller_currency_account, buyer_energy_account, fee_collector, wheeling_collector,
            energy_mint, currency_mint, escrow_authority, market_authority, amount, price,
            wheeling_charge, token_program_id, secondary_token_program_id
        )
    }

    pub fn build_initiate_bridge_transfer_instruction(
        &self,
        market: Pubkey,
        sell_order: Pubkey,
        authority: Pubkey,
        amount: u64,
        target_chain: u16,
        target_address: [u8; 32],
    ) -> Result<Instruction> {
        TradingInstructions::build_initiate_bridge_transfer_instruction(
            market, sell_order, authority, amount, target_chain, target_address
        )
    }

    pub fn build_complete_bridge_transfer_instruction(
        &self,
        authority: Pubkey,
        vaa_hash: [u8; 32],
    ) -> Result<Instruction> {
        TradingInstructions::build_complete_bridge_transfer_instruction(authority, vaa_hash)
    }

    pub fn build_shield_energy_instruction(
        &self,
        confidential_balance: Pubkey,
        mint: Pubkey,
        user_token_account: Pubkey,
        owner: Pubkey,
        amount: u64,
        encrypted_amount: [u8; 64],
        proof_data: [u8; 64],
    ) -> Result<Instruction> {
        TradingInstructions::build_shield_energy_instruction(
            confidential_balance, mint, user_token_account, owner, amount, encrypted_amount, proof_data
        )
    }

    pub fn build_swap_energy_instruction(
        &self,
        pool: Pubkey,
        user_energy: Pubkey,
        user_currency: Pubkey,
        pool_energy_vault: Pubkey,
        pool_currency_vault: Pubkey,
        energy_mint: Pubkey,
        currency_mint: Pubkey,
        user: Pubkey,
        amount_milli_kwh: u64,
        max_currency: u64,
    ) -> Result<Instruction> {
        TradingInstructions::build_swap_energy_instruction(
            pool, user_energy, user_currency, pool_energy_vault, pool_currency_vault,
            energy_mint, currency_mint, user, amount_milli_kwh, max_currency
        )
    }

    // --- Registry/Oracle ---

    pub fn build_initialize_registry_instruction(&self) -> Result<Instruction> {
        RegistryInstructions::build_initialize_registry_instruction(self.payer)
    }

    pub fn build_register_user_instruction(
        &self,
        user_authority: &Pubkey,
        registry: &Pubkey,
        user_type: u8,
        location: &str,
    ) -> Result<Instruction> {
        RegistryInstructions::build_register_user_instruction(
            self.payer, user_authority, registry, user_type, location
        )
    }

    pub fn build_initialize_oracle_instruction(&self, api_gateway: &Pubkey) -> Result<Instruction> {
        RegistryInstructions::build_initialize_oracle_instruction(self.payer, api_gateway)
    }

    pub fn build_update_meter_reading_instruction(
        &self,
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Instruction> {
        RegistryInstructions::build_update_meter_reading_instruction(
            meter_id, produced, consumed, timestamp
        )
    }

    pub fn build_register_meter_instruction(
        &self,
        meter_id: &str,
        meter_type: u8,
    ) -> Result<Instruction> {
        RegistryInstructions::build_register_meter_instruction(self.payer, meter_id, meter_type)
    }

    pub fn build_update_price_instruction(
        &self,
        price_feed_id: &str,
        price: u64,
        confidence: u64,
    ) -> Result<Instruction> {
        RegistryInstructions::build_update_price_instruction(
            self.payer, price_feed_id, price, confidence
        )
    }

    pub fn build_update_registry_instruction(
        &self,
        participant_id: &str,
        update_data: &serde_json::Value,
    ) -> Result<Instruction> {
        // Implementation remains in registry module or delegated
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let participant_account = RegistryInstructions::get_participant_account_pubkey(participant_id)?;
        let accounts = vec![
            AccountMeta::new(participant_account, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]);
        data.extend_from_slice(update_data.to_string().as_bytes());
        Ok(Instruction { program_id, accounts, data })
    }

    pub fn get_user_account_pda(&self, user_authority: &Pubkey) -> Result<Pubkey> {
        RegistryInstructions::get_user_account_pda(user_authority)
    }

    // --- Governance ---

    pub fn build_initialize_governance_instruction(&self) -> Result<Instruction> {
        GovernanceInstructions::build_initialize_governance_instruction(self.payer)
    }

    pub fn build_issue_erc_instruction(
        &self,
        certificate_id: &str,
        user_wallet: &Pubkey,
        meter_account: &Pubkey,
        energy_amount: u64,
        renewable_source: &str,
        validation_data: &str,
    ) -> Result<Instruction> {
        GovernanceInstructions::build_issue_erc_instruction(
            self.payer, certificate_id, meter_account, energy_amount, renewable_source, validation_data
        )
    }

    pub fn build_vote_instruction(&self, proposal_id: u64, vote: bool) -> Result<Instruction> {
        GovernanceInstructions::build_vote_instruction(self.payer, proposal_id, vote)
    }

    pub fn build_transfer_erc_instruction(
        &self,
        certificate_id: &str,
        owner: &Pubkey,
        new_owner: &Pubkey,
    ) -> Result<Instruction> {
        GovernanceInstructions::build_transfer_erc_instruction(certificate_id, owner, new_owner)
    }

    pub fn build_revoke_erc_instruction(
        &self,
        certificate_id: &str,
        reason: &str,
    ) -> Result<Instruction> {
        GovernanceInstructions::build_revoke_erc_instruction(self.payer, certificate_id, reason)
    }

    // --- Tokens ---

    pub fn build_initialize_energy_token_instruction(&self, authority: Pubkey) -> Result<Instruction> {
        TokenInstructions::build_initialize_energy_token_instruction(authority)
    }

    pub fn build_mint_instruction(&self, recipient: &str, amount: u64) -> Result<Instruction> {
        TokenInstructions::build_mint_instruction(self.payer, recipient, amount)
    }

    pub fn build_transfer_instruction(&self, from: &str, to: &str, amount: u64, token_mint: &str) -> Result<Instruction> {
        TokenInstructions::build_transfer_instruction(self.payer, from, to, amount, token_mint)
    }
}

pub mod program_ids {
    use super::*;
    pub use constants::program_ids::*;
}
