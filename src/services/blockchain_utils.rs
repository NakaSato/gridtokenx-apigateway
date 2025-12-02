use anyhow::{Result, anyhow};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;
use tracing::info;

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
        user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Instruction> {
        info!(
            "Creating mint instruction for {} kWh to {}",
            amount_kwh, user_token_account
        );

        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;

        info!("Mint: {}", mint);
        info!("Amount (lamports): {}", amount_lamports);

        let energy_token_program_id = Self::energy_token_program_id()?;

        // Derive token_info PDA
        let (token_info_pda, _) =
            Pubkey::find_program_address(&[b"token_info"], &energy_token_program_id);

        // Build instruction data
        let mut instruction_data = Vec::new();

        // Discriminator for "mint_to_wallet"
        // global:mint_to_wallet = 59e5acb52a926574
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:mint_to_wallet");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);

        // Arguments
        instruction_data.extend_from_slice(&amount_lamports.to_le_bytes());

        // Accounts
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(*mint, false),
            solana_sdk::instruction::AccountMeta::new(token_info_pda, false),
            solana_sdk::instruction::AccountMeta::new(*user_token_account, false),
            solana_sdk::instruction::AccountMeta::new_readonly(*user_wallet, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true), // authority
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),          // payer
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"),
                false,
            ), // Token 2022 program
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
                false,
            ), // Associated Token program
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ), // System program
        ];

        Ok(Instruction::new_with_bytes(
            energy_token_program_id,
            &instruction_data,
            accounts,
        ))
    }

    /// Ensures user has an Associated Token Account for the token mint
    /// Creates ATA if it doesn't exist, returns ATA address
    pub fn create_ata_instruction(
        authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Instruction> {
        // Calculate ATA address manually to avoid type conversion issues
        // ATA = PDA of [associated_token_account_program_id, wallet, token_program_id, mint]
        let ata_program_id = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .map_err(|e| anyhow!("Invalid ATA program ID: {}", e))?;

        let token_program_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .map_err(|e| anyhow!("Invalid token program ID: {}", e))?;

        let (ata_address, _bump) = Pubkey::find_program_address(
            &[
                user_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            &ata_program_id,
        );

        info!("Creating ATA instruction for user: {}", user_wallet);

        // ATA creation instruction data (empty for associated token account creation)
        let instruction_data = vec![];

        // Accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true), // Payer (signer)
            solana_sdk::instruction::AccountMeta::new(ata_address, false), // ATA account (writable)
            solana_sdk::instruction::AccountMeta::new_readonly(*user_wallet, false), // Wallet owner (readonly)
            solana_sdk::instruction::AccountMeta::new_readonly(*mint, false), // Mint (readonly)
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ), // System program
            solana_sdk::instruction::AccountMeta::new_readonly(token_program_id, false), // Token program
        ];

        Ok(Instruction {
            program_id: ata_program_id,
            accounts,
            data: instruction_data,
        })
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
        let token_program_id =
            solana_sdk::pubkey::Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
                .map_err(|e| anyhow!("Invalid token program ID: {}", e))?;

        // Build instruction data for transfer_checked
        // Instruction layout: discriminator(1) + amount(8) + decimals(1) + optional extra
        let mut instruction_data = Vec::with_capacity(10);
        instruction_data.push(3); // transfer_checked instruction discriminator
        instruction_data.extend_from_slice(&amount.to_le_bytes());
        instruction_data.push(decimals);

        // Build accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(*from_token_account, false),
            solana_sdk::instruction::AccountMeta::new(*mint, false),
            solana_sdk::instruction::AccountMeta::new(*to_token_account, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true),
        ];

        Ok(solana_sdk::instruction::Instruction {
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
            .unwrap_or_else(|_| "GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk".to_string());

        program_id
            .parse()
            .map_err(|e| anyhow!("Failed to parse trading program ID: {}", e))
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
        let token_program_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .map_err(|e| anyhow!("Invalid token program ID: {}", e))?;

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
