use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar::clock,
};
use std::str::FromStr;
use super::constants::*;

pub struct RegistryInstructions;

impl RegistryInstructions {
    pub fn build_initialize_registry_instruction(payer: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (registry_pda, _) = Pubkey::find_program_address(&[b"registry"], &program_id);

        let accounts = vec![
            AccountMeta::new(registry_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let data = vec![175, 175, 109, 31, 13, 152, 155, 237];

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_register_user_instruction(
        payer: Pubkey,
        user_authority: &Pubkey,
        registry: &Pubkey,
        user_type: u8,
        location: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (user_account_pda, _) = Pubkey::find_program_address(
            &[b"user", user_authority.as_ref()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(*registry, false),
            AccountMeta::new(user_account_pda, false),
            AccountMeta::new(*user_authority, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[153, 150, 36, 97, 226, 70, 52, 72]);
        data.push(user_type);
        
        let bytes = location.as_bytes();
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_initialize_oracle_instruction(
        payer: Pubkey,
        api_gateway: &Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ORACLE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (oracle_data_pda, _) = Pubkey::find_program_address(&[b"oracle_data"], &program_id);

        let accounts = vec![
            AccountMeta::new(oracle_data_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[175, 175, 109, 31, 13, 152, 155, 237]);
        data.extend_from_slice(api_gateway.as_ref());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_update_price_instruction(
        payer: Pubkey,
        price_feed_id: &str,
        price: u64,
        confidence: u64,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ORACLE_PROGRAM_ID)?;
        let price_feed_account = Self::get_price_feed_account_pubkey(price_feed_id)?;

        let accounts = vec![
            AccountMeta::new(price_feed_account, false),
            AccountMeta::new_readonly(payer, true),
            AccountMeta::new_readonly(clock::id(), false),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]);
        data.extend_from_slice(&price.to_le_bytes());
        data.extend_from_slice(&confidence.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn get_user_account_pda(user_authority: &Pubkey) -> Result<Pubkey> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let (user_account_pda, _) = Pubkey::find_program_address(
            &[b"user", user_authority.as_ref()],
            &program_id,
        );
        Ok(user_account_pda)
    }

    pub fn get_price_feed_account_pubkey(price_feed_id: &str) -> Result<Pubkey> {
        let (price_feed_pubkey, _) = Pubkey::find_program_address(
            &[b"price_feed", price_feed_id.as_bytes()],
            &Pubkey::from_str(ORACLE_PROGRAM_ID)?,
        );
        Ok(price_feed_pubkey)
    }

    pub fn get_participant_account_pubkey(participant_id: &str) -> Result<Pubkey> {
        let (participant_pubkey, _) = Pubkey::find_program_address(
            &[b"participant", participant_id.as_bytes()],
            &Pubkey::from_str(REGISTRY_PROGRAM_ID)?,
        );
        Ok(participant_pubkey)
    }

    pub fn build_update_meter_reading_instruction(
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(ORACLE_PROGRAM_ID)?;
        let (oracle_data_pda, _) = Pubkey::find_program_address(&[b"oracle_data"], &program_id);

        let accounts = vec![
            AccountMeta::new(oracle_data_pda, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[181, 247, 196, 139, 78, 88, 192, 206]);
        
        let bytes = meter_id.as_bytes();
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);
        data.extend_from_slice(&produced.to_le_bytes());
        data.extend_from_slice(&consumed.to_le_bytes());
        data.extend_from_slice(&timestamp.to_le_bytes());

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_register_meter_instruction(
        payer: Pubkey,
        meter_id: &str,
        meter_type: u8,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (registry_pda, _) = Pubkey::find_program_address(&[b"registry"], &program_id);
        let (meter_account_pda, _) = Pubkey::find_program_address(&[b"meter", meter_id.as_bytes()], &program_id);

        let accounts = vec![
            AccountMeta::new(registry_pda, false),
            AccountMeta::new(meter_account_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[94, 154, 252, 175, 41, 143, 27, 21]);
        
        let bytes = meter_id.as_bytes();
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);
        data.push(meter_type);

        Ok(Instruction { program_id, accounts, data })
    }
}
