use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;
use super::constants::*;

pub struct GovernanceInstructions;

impl GovernanceInstructions {
    pub fn build_initialize_governance_instruction(payer: Pubkey) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let data = vec![98, 199, 82, 10, 244, 161, 157, 46];

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_issue_erc_instruction(
        payer: Pubkey,
        certificate_id: &str,
        meter_account: &Pubkey,
        energy_amount: u64,
        renewable_source: &str,
        validation_data: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let system_program = Pubkey::from_str(SYSTEM_PROGRAM_ID)?;

        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);
        let (erc_certificate_pda, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(erc_certificate_pda, false),
            AccountMeta::new(*meter_account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[174, 248, 149, 107, 155, 4, 196, 8]);

        let write_string = |d: &mut Vec<u8>, s: &str| {
            let bytes = s.as_bytes();
            d.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            d.extend_from_slice(bytes);
        };

        write_string(&mut data, certificate_id);
        data.extend_from_slice(&energy_amount.to_le_bytes());
        write_string(&mut data, renewable_source);
        write_string(&mut data, validation_data);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_vote_instruction(payer: Pubkey, proposal_id: u64, vote: bool) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let proposal_account = Self::get_proposal_account_pubkey(proposal_id)?;

        let accounts = vec![
            AccountMeta::new(proposal_account, false),
            AccountMeta::new_readonly(payer, true),
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[1, 0, 0, 0]);
        data.extend_from_slice(&proposal_id.to_le_bytes());
        data.push(if vote { 1 } else { 0 });

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_transfer_erc_instruction(
        certificate_id: &str,
        owner: &Pubkey,
        new_owner: &Pubkey,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);
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

        let mut data = Vec::new();
        data.extend_from_slice(&[200, 15, 16, 13, 13, 143, 11, 11]);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn build_revoke_erc_instruction(
        payer: Pubkey,
        certificate_id: &str,
        reason: &str,
    ) -> Result<Instruction> {
        let program_id = Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?;
        let (poa_config_pda, _) = Pubkey::find_program_address(&[b"poa_config"], &program_id);
        let (erc_certificate_pda, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &program_id,
        );

        let accounts = vec![
            AccountMeta::new(poa_config_pda, false),
            AccountMeta::new(erc_certificate_pda, false),
            AccountMeta::new(payer, true),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&[16, 48, 113, 85, 118, 70, 185, 150]);

        let bytes = reason.as_bytes();
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);

        Ok(Instruction { program_id, accounts, data })
    }

    pub fn get_erc_certificate_pubkey(certificate_id: &str) -> Result<Pubkey> {
        let (certificate_pubkey, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            &Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?,
        );
        Ok(certificate_pubkey)
    }

    pub fn get_proposal_account_pubkey(proposal_id: u64) -> Result<Pubkey> {
        let (proposal_pubkey, _) = Pubkey::find_program_address(
            &[b"proposal", &proposal_id.to_le_bytes()],
            &Pubkey::from_str(GOVERNANCE_PROGRAM_ID)?,
        );
        Ok(proposal_pubkey)
    }
}
