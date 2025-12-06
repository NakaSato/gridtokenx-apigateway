use anyhow::{anyhow, Result};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;
use tracing::info;

// Token Program IDs
#[allow(dead_code)]
const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// Utility functions for Solana blockchain operations
pub struct BlockchainUtils;

impl BlockchainUtils {
    /// Parse Pubkey from string
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        Pubkey::from_str(pubkey_str)
            .map_err(|e| anyhow!("Invalid public key '{}': {}", pubkey_str, e))
    }

    /// Load keypair from a JSON file
    /// The file should contain an array of 64 bytes representing the keypair
    pub fn load_keypair_from_file(filepath: &str) -> Result<Keypair> {
        use std::fs;

        info!("Loading keypair from file: {}", filepath);

        // Read the file contents
        let file_contents = fs::read_to_string(filepath)
            .map_err(|e| anyhow!("Failed to read keypair file '{}': {}", filepath, e))?;

        // Parse the JSON array of bytes
        let bytes: Vec<u8> = serde_json::from_str(&file_contents)
            .map_err(|e| anyhow!("Failed to parse keypair JSON: {}", e))?;

        // Validate the byte array length
        if bytes.len() != 64 {
            return Err(anyhow!(
                "Invalid keypair file: expected 64 bytes, got {}",
                bytes.len()
            ));
        }

        // Convert Vec<u8> to [0u8; 64]
        let mut keypair_bytes = [0u8; 64];
        keypair_bytes.copy_from_slice(&bytes);

        // Create keypair from the secret key (first 32 bytes)
        let mut secret_key = [0u8; 32];
        secret_key.copy_from_slice(&keypair_bytes[0..32]);
        let keypair = Keypair::new_from_array(secret_key);

        info!("Successfully loaded keypair: {}", keypair.pubkey());

        Ok(keypair)
    }

    /// Mint energy tokens directly to a user's token account
    /// This calls the energy_token program's mint_to_wallet instruction
    pub fn create_mint_instruction(
        authority: &Keypair,
        user_token_account: &Pubkey,
        _user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Instruction> {
        info!(
            "Creating mint instruction for {} kWh to {}",
            amount_kwh, user_token_account
        );

        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;

        // Manually build MintTo instruction to bypass spl_token library validation
        // The spl_token library's check_spl_token_program_account() rejects our Token-2022 program ID
        let token_program_id = Self::get_token_program_id()?;

        // MintTo instruction discriminator is 7
        let mut instruction_data = Vec::with_capacity(9);
        instruction_data.push(7); // MintTo discriminator
        instruction_data.extend_from_slice(&amount_lamports.to_le_bytes());

        use solana_sdk::instruction::{AccountMeta, Instruction};

        let mint_instruction = Instruction {
            program_id: token_program_id,
            accounts: vec![
                AccountMeta::new(*mint, false),                      // Mint account
                AccountMeta::new(*user_token_account, false),        // Destination token account
                AccountMeta::new_readonly(authority.pubkey(), true), // Mint authority (signer)
            ],
            data: instruction_data,
        };

        Ok(mint_instruction)
    }

    /// Ensures user has an Associated Token Account for the token mint
    /// Creates ATA if it doesn't exist, returns ATA address
    pub fn create_ata_instruction(
        authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Instruction> {
        info!("Creating ATA instruction for user: {}", user_wallet);

        // Use create_associated_token_account_idempotent to be safer and potentially avoid
        // issues with the standard creation instruction on some validator versions
        // Manual instruction construction to force legacy Create (opcode 0/empty) behavior
        // accounts: [payer, ata, owner, mint, system_program, token_program]
        let derived_ata =
            spl_associated_token_account::get_associated_token_address(user_wallet, mint);

        let account_metas = vec![
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new(derived_ata, false),
            solana_sdk::instruction::AccountMeta::new_readonly(*user_wallet, false),
            solana_sdk::instruction::AccountMeta::new_readonly(*mint, false),
            solana_sdk::instruction::AccountMeta::new_readonly(
                Pubkey::from_str("11111111111111111111111111111111").unwrap(),
                false,
            ),
            solana_sdk::instruction::AccountMeta::new_readonly(
                Self::get_token_program_id()?,
                false,
            ),
        ];

        let instruction = Instruction {
            program_id: spl_associated_token_account::id(),
            accounts: account_metas,
            data: vec![0], // Opcode 0 = Create
        };

        println!(
            "DEBUG: Created manual ATA instruction with Program ID: {}",
            instruction.program_id
        );
        println!("DEBUG: Instruction Data: {:?}", instruction.data);

        Ok(instruction)
    }

    /// Transfer SPL tokens from one account to another
    /// Used for settlement transfers: buyer → seller
    pub fn create_transfer_instruction(
        authority: &Keypair,
        from_token_account: &Pubkey,
        to_token_account: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction> {
        info!(
            "Creating transfer instruction for {} tokens from {} to {}",
            amount, from_token_account, to_token_account
        );

        // Create transfer instruction manually to avoid type conflicts
        let token_program_id = Self::get_token_program_id()?;

        // Build instruction data for transfer_checked
        // Instruction layout: discriminator(1) + amount(8) + decimals(1)
        let mut instruction_data = Vec::with_capacity(10);
        instruction_data.push(12); // transfer_checked instruction discriminator
        instruction_data.extend_from_slice(&amount.to_le_bytes());
        instruction_data.push(decimals);

        info!(
            "TransferChecked Accounts: Source={}, Mint={}, Dest={}, Auth={}",
            from_token_account,
            mint,
            to_token_account,
            authority.pubkey()
        );

        // Build accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(*from_token_account, false),
            solana_sdk::instruction::AccountMeta::new(*mint, false),
            solana_sdk::instruction::AccountMeta::new(*to_token_account, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true),
        ];

        Ok(Instruction {
            program_id: token_program_id,
            accounts,
            data: instruction_data,
        })
    }

    /// Register a user on-chain
    pub fn create_register_user_instruction(
        authority: &Keypair,
        user_type: u8, // 0: Prosumer, 1: Consumer
        location: &str,
    ) -> Result<Instruction> {
        info!(
            "Creating register user instruction for: {}",
            authority.pubkey()
        );

        let registry_program_id = Self::registry_program_id()?;

        // Derive PDAs
        let (registry_pda, _) = Pubkey::find_program_address(&[b"registry"], &registry_program_id);
        let (user_account_pda, _) = Pubkey::find_program_address(
            &[b"user", authority.pubkey().as_ref()],
            &registry_program_id,
        );

        // Build instruction data
        let mut instruction_data = Vec::new();

        // Discriminator for "register_user"
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:register_user");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);

        // Arguments
        instruction_data.push(user_type);
        instruction_data.extend_from_slice(&(location.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(location.as_bytes());

        // Accounts
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(registry_pda, false),
            solana_sdk::instruction::AccountMeta::new(user_account_pda, false),
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ),
        ];

        Ok(Instruction::new_with_bytes(
            registry_program_id,
            &instruction_data,
            accounts,
        ))
    }

    /// Register a meter on-chain
    pub fn create_register_meter_instruction(
        authority: &Keypair,
        meter_id: &str,
        meter_type: u8, // 0: Solar, 1: Wind, etc.
    ) -> Result<Instruction> {
        info!("Creating register meter instruction for: {}", meter_id);

        let registry_program_id = Self::registry_program_id()?;

        // Derive PDAs
        let (registry_pda, _) = Pubkey::find_program_address(&[b"registry"], &registry_program_id);
        let (user_account_pda, _) = Pubkey::find_program_address(
            &[b"user", authority.pubkey().as_ref()],
            &registry_program_id,
        );
        let (meter_account_pda, _) =
            Pubkey::find_program_address(&[b"meter", meter_id.as_bytes()], &registry_program_id);

        // Build instruction data
        let mut instruction_data = Vec::new();

        // Discriminator for "register_meter"
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:register_meter");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);

        // Arguments
        instruction_data.extend_from_slice(&(meter_id.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(meter_id.as_bytes());
        instruction_data.push(meter_type);

        // Accounts
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(registry_pda, false),
            solana_sdk::instruction::AccountMeta::new(user_account_pda, false),
            solana_sdk::instruction::AccountMeta::new(meter_account_pda, false),
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ),
        ];

        Ok(Instruction::new_with_bytes(
            registry_program_id,
            &instruction_data,
            accounts,
        ))
    }

    /// Submit meter reading on-chain (via Oracle)
    pub fn create_submit_meter_reading_instruction(
        authority: &Keypair, // Must be API Gateway authority
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Instruction> {
        info!(
            "Creating submit meter reading instruction for: {}",
            meter_id
        );

        let oracle_program_id = Self::oracle_program_id()?;
        let registry_program_id = Self::registry_program_id()?;

        // Derive PDAs
        let (oracle_data_pda, _) =
            Pubkey::find_program_address(&[b"oracle_data"], &oracle_program_id);
        let (_meter_account_pda, _) =
            Pubkey::find_program_address(&[b"meter", meter_id.as_bytes()], &registry_program_id);

        // Build instruction data
        let mut instruction_data = Vec::new();

        // Use discriminator from IDL for submit_meter_reading: [181, 247, 196, 139, 78, 88, 192, 206]
        instruction_data.extend_from_slice(&[181, 247, 196, 139, 78, 88, 192, 206]);

        // Arguments
        instruction_data.extend_from_slice(&(meter_id.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(meter_id.as_bytes());
        instruction_data.extend_from_slice(&produced.to_le_bytes());
        instruction_data.extend_from_slice(&consumed.to_le_bytes());
        instruction_data.extend_from_slice(&timestamp.to_le_bytes());

        // Accounts - matching IDL (oracle_data, authority)
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(oracle_data_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true),
        ];

        Ok(Instruction::new_with_bytes(
            oracle_program_id,
            &instruction_data,
            accounts,
        ))
    }

    /// Burn energy tokens (for energy consumption)
    pub fn create_burn_instruction(
        authority: &Keypair,
        user_token_account: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Instruction> {
        info!(
            "Creating burn instruction for {} kWh from {}",
            amount_kwh, user_token_account
        );

        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh.abs() * 1_000_000_000.0) as u64;

        let energy_token_program_id = Self::energy_token_program_id()?;

        // Derive token_info PDA
        let (token_info_pda, _) =
            Pubkey::find_program_address(&[b"token_info"], &energy_token_program_id);

        // Build instruction data
        let mut instruction_data = Vec::new();

        // Discriminator for "burn_tokens"
        // global:burn_tokens = 9b4d08130831626e
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:burn_tokens");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);

        // Arguments
        instruction_data.extend_from_slice(&amount_lamports.to_le_bytes());

        // Accounts
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(token_info_pda, false),
            solana_sdk::instruction::AccountMeta::new(*mint, false),
            solana_sdk::instruction::AccountMeta::new(*user_token_account, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(
                Self::get_token_program_id()?,
                false,
            ), // Token program
        ];

        Ok(Instruction::new_with_bytes(
            energy_token_program_id,
            &instruction_data,
            accounts,
        ))
    }

    // Helper methods for program IDs

    /// Get Registry program ID
    fn registry_program_id() -> Result<Pubkey> {
        let program_id = std::env::var("REGISTRY_PROGRAM_ID")
            .unwrap_or_else(|_| "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse registry program ID: {}", e))
    }

    /// Get Oracle program ID
    fn oracle_program_id() -> Result<Pubkey> {
        let program_id = std::env::var("ORACLE_PROGRAM_ID")
            .unwrap_or_else(|_| "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse oracle program ID: {}", e))
    }

    /// Get Governance program ID
    #[allow(dead_code)]
    fn governance_program_id() -> Result<Pubkey> {
        let program_id = std::env::var("GOVERNANCE_PROGRAM_ID")
            .unwrap_or_else(|_| "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse governance program ID: {}", e))
    }

    /// Get Energy Token program ID
    fn energy_token_program_id() -> Result<Pubkey> {
        let program_id = std::env::var("ENERGY_TOKEN_PROGRAM_ID")
            .unwrap_or_else(|_| "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse energy token program ID: {}", e))
    }

    /// Get Trading program ID
    #[allow(dead_code)]
    fn trading_program_id() -> Result<Pubkey> {
        let program_id = std::env::var("TRADING_PROGRAM_ID")
            .unwrap_or_else(|_| "9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse trading program ID: {}", e))
    }

    /// Get the correct Token Program ID
    /// For now, we use Token-2022 since our mint is deployed with Token-2022
    /// In a production system, you would query the mint account to detect its owner
    pub fn get_token_program_id() -> Result<Pubkey> {
        // Use Token-2022 program ID since our energy token mint uses Token-2022
        Pubkey::from_str(TOKEN_2022_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse token program ID: {}", e))
    }
}

/// Helper functions for transaction building
pub mod transaction_utils {
    use super::*;
    use solana_sdk::hash::Hash;
    use solana_sdk::instruction::Instruction;
    use solana_sdk::signature::Keypair;
    use solana_sdk::transaction::Transaction;

    /// Build a transaction from instructions
    pub fn build_transaction(
        instructions: Vec<Instruction>,
        payer: &Pubkey,
        _recent_blockhash: Hash,
    ) -> Transaction {
        Transaction::new_with_payer(&instructions, Some(payer))
    }

    /// Sign a transaction
    pub fn sign_transaction(
        transaction: &mut Transaction,
        signers: &[&Keypair],
        recent_blockhash: Hash,
    ) -> Result<()> {
        transaction
            .try_sign(signers, recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;
        Ok(())
    }

    /// Data structure for batch minting operations
    #[derive(Debug, Clone)]
    pub struct MintBatchData {
        pub user_wallet: Pubkey,
        pub user_token_account: Pubkey,
        pub amount_kwh: f64,
        pub tokens_to_mint: u64,
    }

    /// Result of a batch minting operation
    #[derive(Debug, Clone)]
    pub struct MintBatchResult {
        pub user_wallet: Pubkey,
        pub success: bool,
        pub error: Option<String>,
        pub tx_signature: Option<String>,
    }

    /// Helper method to create MintBatchData from user wallet and kWh amount
    pub fn create_mint_batch_data(
        user_wallet: Pubkey,
        kwh_amount: f64,
        kwh_to_token_ratio: f64,
        decimals: u8,
    ) -> Result<MintBatchData> {
        // Calculate tokens to mint
        let tokens_to_mint =
            (kwh_amount * kwh_to_token_ratio * 10_f64.powi(decimals as i32)) as u64;

        // Get or create associated token account
        let token_program_id = BlockchainUtils::get_token_program_id()?;

        let ata_program_id = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .map_err(|e| anyhow!("Invalid ATA program ID: {}", e))?;

        // Get energy token mint
        let mint = Pubkey::from_str(
            &std::env::var("ENERGY_TOKEN_MINT")
                .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?,
        )?;

        // Calculate ATA address
        let (user_token_account, _bump) = Pubkey::find_program_address(
            &[
                user_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            &ata_program_id,
        );

        Ok(MintBatchData {
            user_wallet,
            user_token_account,
            amount_kwh: kwh_amount,
            tokens_to_mint,
        })
    }
}
