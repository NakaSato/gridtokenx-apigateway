use anyhow::{anyhow, Result};
use solana_sdk::sysvar::clock;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;

// System program ID constant
const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";

/// Program IDs (localnet) — keep in sync with `gridtokenx-anchor/Anchor.toml`
pub const REGISTRY_PROGRAM_ID: &str = "HWoKSbNy4jJBFJ7g7drxZgAfTmjFqvg1Sx6vXosfJNAi";
pub const ORACLE_PROGRAM_ID: &str = "5z6Qaf6UUv42uCqbxQLfKz7cSXhMABsq73mRMwvHKzFA";
pub const GOVERNANCE_PROGRAM_ID: &str = "2WrMSfreZvCCKdQMQGY7bTFgXKgr42fYipJR6VXn1Q8c";
pub const ENERGY_TOKEN_PROGRAM_ID: &str = "MwAdshY2978VqcpJzWSKmPfDtKfweD7YLMCQSBcR4wP";
pub const TRADING_PROGRAM_ID: &str = "Fmk6vb74MjZpXVE9kAS5q4U5L8hr2AEJcDikfRSFTiyY";

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
        market_pubkey: &Pubkey,
        _authority: &Pubkey,
        order_pda: Pubkey,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
        payer: Pubkey,
    ) -> Result<Instruction> {
        // Parse program and market pubkeys
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        // market_pubkey is already Pubkey

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
                AccountMeta::new(*market_pubkey, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new_readonly(erc_key, false),
                AccountMeta::new(payer, true), // Payer must be writable to pay for rent
                AccountMeta::new_readonly(system_program, false),
            ]
        } else {
            // Buy orders do NOT have the ERC certificate account
            // IDL: market, order, authority, systemProgram
            vec![
                AccountMeta::new(*market_pubkey, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new(payer, true), // Payer must be writable to pay for rent
                AccountMeta::new_readonly(system_program, false),
            ]
        };

        // Build instruction data
        let mut data = Vec::new();

        // Add instruction discriminator based on order type (Anchor uses 8-byte sha256("global:<name>"))
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
            AccountMeta::new_readonly(Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?, false),
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
            AccountMeta::new_readonly(Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?, false),
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

    /// Build instruction for initializing the registry
    pub fn build_initialize_registry_instruction(&self) -> Result<Instruction> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find registry PDA: seeds = ["registry"]
        let (registry_pda, _bump) = Pubkey::find_program_address(&[b"registry"], &program_id);

        let accounts = vec![
            AccountMeta::new(registry_pda, false),
            AccountMeta::new(self.payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        // initialize discriminator: [175, 175, 109, 31, 13, 152, 155, 237]
        let mut data = Vec::new();
        data.extend_from_slice(&[175, 175, 109, 31, 13, 152, 155, 237]);

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for initializing the oracle
    pub fn build_initialize_oracle_instruction(
        &self,
        api_gateway: &Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ORACLE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find oracle_data PDA: seeds = ["oracle_data"]
        let (oracle_data_pda, _bump) = Pubkey::find_program_address(&[b"oracle_data"], &program_id);

        let accounts = vec![
            AccountMeta::new(oracle_data_pda, false),
            AccountMeta::new(self.payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        // initialize discriminator: [175, 175, 109, 31, 13, 152, 155, 237]
        let mut data = Vec::new();
        data.extend_from_slice(&[175, 175, 109, 31, 13, 152, 155, 237]);
        data.extend_from_slice(api_gateway.as_ref());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for initializing the governance (PoA)
    pub fn build_initialize_governance_instruction(&self) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find poa_config PDA: seeds = ["poa_config"]
        let (poa_config_pda, _bump) = Pubkey::find_program_address(&[b"poa_config"], &program_id);

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(self.payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        // initialize_poa discriminator: [98, 199, 82, 10, 244, 161, 157, 46]
        let mut data = Vec::new();
        data.extend_from_slice(&[98, 199, 82, 10, 244, 161, 157, 46]);

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for issuing an ERC certificate
    pub fn build_issue_erc_instruction(
        &self,
        certificate_id: &str,
        _user_wallet: &Pubkey,
        meter_account: &Pubkey,
        energy_amount: u64,
        renewable_source: &str,
        validation_data: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find poa_config PDA: seeds = ["poa_config"]
        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);

        // Find erc_certificate PDA: seeds = ["erc_certificate", certificate_id]
        let (erc_certificate_pda, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(erc_certificate_pda, false),
            AccountMeta::new(*meter_account, false),
            AccountMeta::new(self.payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        // issue_erc discriminator: [174, 248, 149, 107, 155, 4, 196, 8]
        let mut data = Vec::new();
        data.extend_from_slice(&[174, 248, 149, 107, 155, 4, 196, 8]);

        // Args: certificate_id (String), energy_amount (u64), renewable_source (String), validation_data (String)
        let write_string = |d: &mut Vec<u8>, s: &str| {
            let bytes = s.as_bytes();
            d.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            d.extend_from_slice(bytes);
        };

        write_string(&mut data, certificate_id);
        data.extend_from_slice(&energy_amount.to_le_bytes());
        write_string(&mut data, renewable_source);
        write_string(&mut data, validation_data);

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for transferring an ERC certificate
    pub fn build_transfer_erc_instruction(
        &self,
        certificate_id: &str,
        owner: &Pubkey,
        new_owner: &Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;

        // Find poa_config PDA
        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);

        // Find erc_certificate PDA
        let (erc_certificate_pda, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(erc_certificate_pda, false),
            AccountMeta::new(*owner, true),
            AccountMeta::new_readonly(*new_owner, false),
        ];

        // transfer_erc discriminator: [200, 15, 16, 13, 13, 143, 11, 11]
        let mut data = Vec::new();
        data.extend_from_slice(&[200, 15, 16, 13, 13, 143, 11, 11]);

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for revoking (retiring) an ERC certificate
    pub fn build_revoke_erc_instruction(
        &self,
        certificate_id: &str,
        reason: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;

        // Find poa_config PDA
        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);

        // Find erc_certificate PDA
        let (erc_certificate_pda, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(erc_certificate_pda, false),
            AccountMeta::new(self.payer, true),
        ];

        // revoke_erc discriminator: [16, 48, 113, 85, 118, 70, 185, 150]
        let mut data = Vec::new();
        data.extend_from_slice(&[16, 48, 113, 85, 118, 70, 185, 150]);

        // Arg: reason (String)
        let bytes = reason.as_bytes();
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);

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
        let mint_str = std::env::var("ENERGY_TOKEN_MINT")
            .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?;
            
        Pubkey::from_str(&mint_str)
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

    /// Build instruction for registering a user in the registry program
    /// This creates an on-chain PDA account at ["user", user_authority]
    pub fn build_register_user_instruction(
        &self,
        user_authority: &Pubkey,
        registry: &Pubkey,
        user_type: u8,        // 0=Consumer, 1=Producer, 2=Prosumer
        location: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find user account PDA: seeds = ["user", user_authority]
        let (user_account_pda, _bump) = Pubkey::find_program_address(
            &[b"user", user_authority.as_ref()],
            &program_id,
        );

        // Build accounts array matching RegisterUser struct
        let accounts = vec![
            AccountMeta::new(*registry, false),              // registry (mut)
            AccountMeta::new(user_account_pda, false),       // user_account (init, mut)
            AccountMeta::new(*user_authority, true),         // user_authority (signer, mut)
            AccountMeta::new_readonly(system_program, false), // system_program
        ];

        // Build instruction data
        let mut data = Vec::new();
        
        // register_user discriminator: sha256("global:register_user")[0..8]
        // Computed: [153, 150, 36, 97, 226, 70, 52, 72]
        data.extend_from_slice(&[153, 150, 36, 97, 226, 70, 52, 72]);
        
        // UserType enum (1 byte)
        data.push(user_type);
        
        // Location string (length prefix + bytes)
        let location_bytes = location.as_bytes();
        data.extend_from_slice(&(location_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(location_bytes);

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Get user account PDA from user authority
    pub fn get_user_account_pda(&self, user_authority: &Pubkey) -> Result<Pubkey> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let (user_account_pda, _) = Pubkey::find_program_address(
            &[b"user", user_authority.as_ref()],
            &program_id,
        );
        Ok(user_account_pda)
    }

    /// Build instruction for initializing the Energy Token program
    pub fn build_initialize_energy_token_instruction(&self, authority: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?; // Token-2022
        let rent = solana_sdk::sysvar::rent::ID;

        // PDAs
        let (token_info_pda, _) = Pubkey::find_program_address(&[b"token_info_2022"], &program_id);
        let (mint_pda, _) = Pubkey::find_program_address(&[b"mint_2022"], &program_id);

        let accounts = vec![
            AccountMeta::new(token_info_pda, false),
            AccountMeta::new(mint_pda, false),
            AccountMeta::new(authority, true), // authority used as payer
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(rent, false),
        ];

        // Discriminator for "initialize_token" (from IDL: [38, 209, 150, 50, 190, 117, 16, 54])
        let data = vec![38, 209, 150, 50, 190, 117, 16, 54];

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for initializing the Trading Market
    pub fn build_initialize_market_instruction(&self, authority: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        // Find market PDA: seeds = ["market"]
        let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &program_id);

        let accounts = vec![
            AccountMeta::new(market_pda, false),
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        // discriminator: [35, 35, 189, 193, 155, 48, 170, 203]
        let data = vec![35, 35, 189, 193, 155, 48, 170, 203];

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for executing a truly atomic settlement
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
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let accounts = vec![
            AccountMeta::new(market, false),
            AccountMeta::new(buy_order, false),
            AccountMeta::new(sell_order, false),
            AccountMeta::new(buyer_currency_escrow, false),
            AccountMeta::new(seller_energy_escrow, false),
            AccountMeta::new(seller_currency_account, false),
            AccountMeta::new(buyer_energy_account, false),
            AccountMeta::new(fee_collector, false),
            AccountMeta::new(wheeling_collector, false),
            AccountMeta::new_readonly(energy_mint, false),
            AccountMeta::new_readonly(currency_mint, false),
            AccountMeta::new_readonly(escrow_authority, true),
            AccountMeta::new_readonly(market_authority, true),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(secondary_token_program_id, false),
        ];

        // discriminator: [86, 216, 13, 114, 76, 114, 212, 11]
        let mut data = Vec::new();
        data.extend_from_slice(&[86, 216, 13, 114, 76, 114, 212, 11]);
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&price.to_le_bytes());
        data.extend_from_slice(&wheeling_charge.to_le_bytes());

        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
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
