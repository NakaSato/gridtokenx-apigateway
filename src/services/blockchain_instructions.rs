use anyhow::{anyhow, Result};
use solana_sdk::sysvar::clock;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;

// System program ID constant
const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";

/// Program IDs (localnet) — keep in sync with `gridtokenx-anchor/Anchor.toml`
pub const REGISTRY_PROGRAM_ID: &str = "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7";
pub const ORACLE_PROGRAM_ID: &str = "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE";
pub const GOVERNANCE_PROGRAM_ID: &str = "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe";
pub const ENERGY_TOKEN_PROGRAM_ID: &str = "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur";
pub const TRADING_PROGRAM_ID: &str = "9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb";

/// Instruction builder for Solana programs
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

    /// Build instruction for creating energy trade order
    pub fn build_create_order_instruction(
        &self,
        market_pubkey: &str,
        order_pda: Pubkey,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
        payer: Pubkey,
    ) -> Result<Instruction> {
        // Parse program and market pubkeys
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let market = Pubkey::from_str(market_pubkey)?;

        // Find ERC certificate account if provided
        let erc_certificate = if let Some(cert_id) = erc_certificate_id {
            Some(self.get_erc_certificate_pubkey(cert_id)?)
        } else {
            None
        };

        // Build accounts array
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let accounts = if order_type == "sell" {
            // Sell orders have an optional ERC certificate account at index 2
            let erc_key = erc_certificate.unwrap_or(program_id);
            vec![
                AccountMeta::new(market, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new_readonly(erc_key, false),
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ]
        } else {
            // Buy orders do NOT have the ERC certificate account
            // IDL: market, order, authority, systemProgram
            vec![
                AccountMeta::new(market, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new_readonly(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ]
        };

        // Build instruction data
        let mut data = Vec::new();

        // Add instruction discriminator based on order type
        if order_type == "sell" {
            // createSellOrder discriminator: [53, 52, 255, 44, 191, 74, 171, 225]
            data.extend_from_slice(&[53, 52, 255, 44, 191, 74, 171, 225]);
        } else {
            // createBuyOrder discriminator: [182, 87, 0, 160, 192, 66, 151, 130]
            data.extend_from_slice(&[182, 87, 0, 160, 192, 66, 151, 130]);
        }

        // Add parameters
        data.extend_from_slice(&energy_amount.to_le_bytes());
        data.extend_from_slice(&price_per_kwh.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for matching orders
    pub fn build_match_orders_instruction(
        &self,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
        trade_record_pubkey: Pubkey,
    ) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let market = Pubkey::from_str(market_pubkey)?;
        let buy_order = Pubkey::from_str(buy_order_pubkey)?;
        let sell_order = Pubkey::from_str(sell_order_pubkey)?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(market, false),
            AccountMeta::new(buy_order, false),
            AccountMeta::new(sell_order, false),
            AccountMeta::new(trade_record_pubkey, false), // PDA doesn't sign, Anchor verifies seeds
            AccountMeta::new(self.payer, true), // Changed to mut - payer pays for trade_record init
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        // MatchOrders discriminator: [17, 1, 201, 93, 7, 51, 251, 134]
        data.extend_from_slice(&[17, 1, 201, 93, 7, 51, 251, 134]);
        data.extend_from_slice(&match_amount.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for minting tokens
    pub fn build_mint_instruction(&self, recipient: &str, amount: u64) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let recipient_pubkey = Pubkey::from_str(recipient)?;
        let mint_pubkey = self.get_token_mint_pubkey()?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(recipient_pubkey, false),
            AccountMeta::new(mint_pubkey, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(spl_token::ID, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]); // Mint discriminator
        data.extend_from_slice(&amount.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for transferring tokens
    pub fn build_transfer_instruction(
        &self,
        from: &str,
        to: &str,
        amount: u64,
        token_mint: &str,
    ) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let from_pubkey = Pubkey::from_str(from)?;
        let to_pubkey = Pubkey::from_str(to)?;
        let mint_pubkey = Pubkey::from_str(token_mint)?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(from_pubkey, false),
            AccountMeta::new(to_pubkey, false),
            AccountMeta::new(mint_pubkey, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(spl_token::ID, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&[2, 0, 0, 0]); // Transfer discriminator
        data.extend_from_slice(&amount.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for casting a governance vote
    pub fn build_vote_instruction(&self, proposal_id: u64, vote: bool) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let proposal_account = self.get_proposal_account_pubkey(proposal_id)?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(proposal_account, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]); // Vote discriminator
        data.extend_from_slice(&proposal_id.to_le_bytes());
        data.push(if vote { 1 } else { 0 });

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for updating oracle price
    pub fn build_update_price_instruction(
        &self,
        price_feed_id: &str,
        price: u64,
        confidence: u64,
    ) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(ORACLE_PROGRAM_ID)?;
        let price_feed_account = self.get_price_feed_account_pubkey(price_feed_id)?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(price_feed_account, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(clock::id(), false),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]); // UpdatePrice discriminator
        data.extend_from_slice(&price.to_le_bytes());
        data.extend_from_slice(&confidence.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for updating registry
    pub fn build_update_registry_instruction(
        &self,
        participant_id: &str,
        update_data: &serde_json::Value,
    ) -> Result<Instruction> {
        // Parse pubkeys
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let participant_account = self.get_participant_account_pubkey(participant_id)?;

        // Build accounts array
        let accounts = vec![
            AccountMeta::new(participant_account, false),
            AccountMeta::new_readonly(self.payer, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]); // UpdateParticipant discriminator
        data.extend_from_slice(update_data.to_string().as_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    // Helper methods

    /// Get ERC certificate pubkey from certificate ID
    fn get_erc_certificate_pubkey(&self, certificate_id: &str) -> Result<Pubkey> {
        let (certificate_pubkey, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?,
        );

        Ok(certificate_pubkey)
    }

    /// Get token mint pubkey
    fn get_token_mint_pubkey(&self) -> Result<Pubkey> {
        // In a real implementation, this would be configured or derived
        "GRXTokenMint11111111111111111111111111"
            .parse()
            .map_err(|e| anyhow!("Failed to parse token mint pubkey: {}", e))
    }

    /// Get proposal account pubkey from proposal ID
    fn get_proposal_account_pubkey(&self, proposal_id: u64) -> Result<Pubkey> {
        let (proposal_pubkey, _) = Pubkey::find_program_address(
            &[b"proposal", &proposal_id.to_le_bytes()],
            &Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?,
        );

        Ok(proposal_pubkey)
    }

    /// Get price feed account pubkey from price feed ID
    fn get_price_feed_account_pubkey(&self, price_feed_id: &str) -> Result<Pubkey> {
        let (price_feed_pubkey, _) = Pubkey::find_program_address(
            &[b"price_feed", price_feed_id.as_bytes()],
            &Pubkey::from_str(ORACLE_PROGRAM_ID)?,
        );

        Ok(price_feed_pubkey)
    }

    /// Get participant account pubkey from participant ID
    fn get_participant_account_pubkey(&self, participant_id: &str) -> Result<Pubkey> {
        let (participant_pubkey, _) = Pubkey::find_program_address(
            &[b"participant", participant_id.as_bytes()],
            &Pubkey::from_str(REGISTRY_PROGRAM_ID)?,
        );

        Ok(participant_pubkey)
    }
}

/// Program ID utilities
pub mod program_ids {
    use super::*;
    use anyhow::Result;

    /// Get Registry program ID
    pub fn registry_program_id() -> Result<Pubkey> {
        Pubkey::from_str(REGISTRY_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse registry program ID: {}", e))
    }

    /// Get Oracle program ID
    pub fn oracle_program_id() -> Result<Pubkey> {
        Pubkey::from_str(ORACLE_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse oracle program ID: {}", e))
    }

    /// Get Governance program ID
    pub fn governance_program_id() -> Result<Pubkey> {
        Pubkey::from_str(GOVERNANCE_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse governance program ID: {}", e))
    }

    /// Get Energy Token program ID
    pub fn energy_token_program_id() -> Result<Pubkey> {
        Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse energy token program ID: {}", e))
    }

    /// Get Trading program ID
    pub fn trading_program_id() -> Result<Pubkey> {
        Pubkey::from_str(TRADING_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse trading program ID: {}", e))
    }
}
