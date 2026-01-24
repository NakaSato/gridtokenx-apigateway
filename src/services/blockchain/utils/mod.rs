pub mod keys;
pub mod pda;
pub mod math;

pub use keys::KeyUtils;
pub use pda::PdaUtils;
pub use math::TokenMath;

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use crate::services::blockchain::instructions::tokens::TokenInstructions;

/// Proxy struct for backward compatibility
pub struct BlockchainUtils;

impl BlockchainUtils {
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        KeyUtils::parse_pubkey(pubkey_str)
    }

    pub fn load_keypair_from_file(filepath: &str) -> Result<Keypair> {
        KeyUtils::load_keypair_from_file(filepath)
    }

    pub fn kwh_to_lamports(amount_kwh: f64) -> u64 {
        TokenMath::kwh_to_lamports(amount_kwh)
    }

    pub fn get_token_program_id() -> Result<Pubkey> {
        TokenInstructions::get_token_program_id()
    }

    pub fn create_burn_instruction(
        authority: &Keypair,
        user_token_account: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<solana_sdk::instruction::Instruction> {
        let amount_lamports = Self::kwh_to_lamports(amount_kwh);
        TokenInstructions::build_burn_instruction(authority.pubkey(), *user_token_account, *mint, amount_lamports)
    }

    pub fn create_transfer_instruction(
        authority: &Keypair,
        from: &Pubkey,
        to: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<solana_sdk::instruction::Instruction> {
        TokenInstructions::build_spl_transfer_instruction(authority.pubkey(), *from, *to, *mint, amount, decimals)
    }
}
