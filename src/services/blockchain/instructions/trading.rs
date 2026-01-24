use anyhow::{anyhow, Result};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;
use super::constants::*;

pub struct TradingInstructions;

impl TradingInstructions {
    /// Build instruction for creating energy trade order
    pub fn build_create_order_instruction(
        market_pubkey: &Pubkey,
        order_pda: Pubkey,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_pda: Option<Pubkey>,
        payer: Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let accounts = if order_type == "sell" {
            let erc_key = erc_certificate_pda.unwrap_or(program_id);
            vec![
                AccountMeta::new(*market_pubkey, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new_readonly(erc_key, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ]
        } else {
            vec![
                AccountMeta::new(*market_pubkey, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ]
        };

        let mut data = Vec::new();
        if order_type == "sell" {
            data.extend_from_slice(&[53, 52, 255, 44, 191, 74, 171, 225]);
        } else {
            data.extend_from_slice(&[182, 87, 0, 160, 192, 66, 151, 130]);
        }

        data.extend_from_slice(&energy_amount.to_le_bytes());
        data.extend_from_slice(&price_per_kwh.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    /// Build instruction for matching orders
    pub fn build_match_orders_instruction(
        payer: Pubkey,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
        trade_record_pubkey: Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let market = Pubkey::from_str(market_pubkey)?;
        let buy_order = Pubkey::from_str(buy_order_pubkey)?;
        let sell_order = Pubkey::from_str(sell_order_pubkey)?;

        let accounts = vec![
            AccountMeta::new(market, false),
            AccountMeta::new(buy_order, false),
            AccountMeta::new(sell_order, false),
            AccountMeta::new(trade_record_pubkey, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[17, 1, 201, 93, 7, 51, 251, 134]);
        data.extend_from_slice(&match_amount.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_initialize_market_instruction(authority: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &program_id);

        let accounts = vec![
            AccountMeta::new(market_pda, false),
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let data = vec![35, 35, 189, 193, 155, 48, 170, 203];

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_execute_atomic_settlement_instruction(
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

        let mut data = Vec::new();
        data.extend_from_slice(&[86, 216, 13, 114, 76, 114, 212, 11]);
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&price.to_le_bytes());
        data.extend_from_slice(&wheeling_charge.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_shield_energy_instruction(
        confidential_balance: Pubkey,
        mint: Pubkey,
        user_token_account: Pubkey,
        owner: Pubkey,
        amount: u64,
        encrypted_amount: [u8; 64],
        proof_data: [u8; 64],
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        
        let accounts = vec![
            AccountMeta::new(confidential_balance, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(user_token_account, false),
            AccountMeta::new(owner, true),
            AccountMeta::new_readonly(token_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[18, 113, 101, 142, 63, 11, 252, 178]);
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&encrypted_amount);
        data.extend_from_slice(&proof_data);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_swap_energy_instruction(
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
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;

        let accounts = vec![
            AccountMeta::new(pool, false),
            AccountMeta::new(user_energy, false),
            AccountMeta::new(user_currency, false),
            AccountMeta::new(pool_energy_vault, false),
            AccountMeta::new(pool_currency_vault, false),
            AccountMeta::new_readonly(energy_mint, false),
            AccountMeta::new_readonly(currency_mint, false),
            AccountMeta::new_readonly(user, true),
            AccountMeta::new_readonly(token_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[173, 218, 178, 117, 111, 237, 240, 16]);
        data.extend_from_slice(&amount_milli_kwh.to_le_bytes());
        data.extend_from_slice(&max_currency.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_initiate_bridge_transfer_instruction(
        market: Pubkey,
        sell_order: Pubkey,
        authority: Pubkey,
        amount: u64,
        target_chain: u16,
        target_address: [u8; 32],
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let accounts = vec![
            AccountMeta::new(market, false),
            AccountMeta::new(sell_order, false),
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[181, 100, 216, 14, 219, 149, 169, 185]);
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&target_chain.to_le_bytes());
        data.extend_from_slice(&target_address);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_complete_bridge_transfer_instruction(
        authority: Pubkey,
        _vaa_hash: [u8; 32],
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let accounts = vec![
            AccountMeta::new(authority, true),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[141, 172, 85, 238, 118, 149, 19, 188]);
        Ok(Instruction { program_id, accounts, data })
    }
}
