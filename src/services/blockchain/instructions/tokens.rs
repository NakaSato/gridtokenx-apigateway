use anyhow::{anyhow, Result};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;
use super::constants::*;

pub struct TokenInstructions;

impl TokenInstructions {
    pub fn build_initialize_energy_token_instruction(authority: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        let rent = solana_sdk::sysvar::rent::ID;

        let (token_info_pda, _) = Pubkey::find_program_address(&[b"token_info_2022"], &program_id);
        let (mint_pda, _) = Pubkey::find_program_address(&[b"mint_2022"], &program_id);

        let accounts = vec![
            AccountMeta::new(token_info_pda, false),
            AccountMeta::new(mint_pda, false),
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(rent, false),
        ];

        let data = vec![38, 209, 150, 50, 190, 117, 16, 54];

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_mint_instruction(payer: Pubkey, recipient: &str, amount: u64) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let recipient_pubkey = Pubkey::from_str(recipient)?;
        let mint_pubkey = Self::get_token_mint_pubkey()?;

        let accounts = vec![
            AccountMeta::new(recipient_pubkey, false),
            AccountMeta::new(mint_pubkey, false),
            AccountMeta::new_readonly(payer, true),
            AccountMeta::new_readonly(Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]);
        data.extend_from_slice(&amount.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_transfer_instruction(
        payer: Pubkey,
        from: &str,
        to: &str,
        amount: u64,
        token_mint: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let from_pubkey = Pubkey::from_str(from)?;
        let to_pubkey = Pubkey::from_str(to)?;
        let mint_pubkey = Pubkey::from_str(token_mint)?;

        let accounts = vec![
            AccountMeta::new(from_pubkey, false),
            AccountMeta::new(to_pubkey, false),
            AccountMeta::new(mint_pubkey, false),
            AccountMeta::new_readonly(payer, true),
            AccountMeta::new_readonly(Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[2, 0, 0, 0]);
        data.extend_from_slice(&amount.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn get_token_mint_pubkey() -> Result<Pubkey> {
        let mint_str = std::env::var("ENERGY_TOKEN_MINT")
            .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?;
            
        Pubkey::from_str(&mint_str)
            .map_err(|e| anyhow!("Failed to parse token mint pubkey: {}", e))
    }
    pub fn get_token_program_id() -> Result<Pubkey> {
        Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .map_err(|e| anyhow!("Failed to parse token program ID: {}", e))
    }

    pub fn build_burn_instruction(
        authority: Pubkey,
        user_token_account: Pubkey,
        mint: Pubkey,
        amount_lamports: u64,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)?;
        let (token_info_pda, _) = Pubkey::find_program_address(&[b"token_info_2022"], &program_id);

        let accounts = vec![
            AccountMeta::new(token_info_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(user_token_account, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(Self::get_token_program_id()?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[155, 77, 8, 19, 8, 49, 98, 110]);
        data.extend_from_slice(&amount_lamports.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_spl_transfer_instruction(
        authority: Pubkey,
        from: Pubkey,
        to: Pubkey,
        mint: Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction> {
        let token_program_id = Self::get_token_program_id()?;
        let instruction = spl_token::instruction::transfer_checked(
            &token_program_id,
            &from,
            &mint,
            &to,
            &authority,
            &[],
            amount,
            decimals,
        )?;
        Ok(instruction)
    }
}
