use crate::services::priority_fee_service::{PriorityFeeService, TransactionType};
use anyhow::{Result, anyhow};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Program IDs (localnet) — keep in sync with `gridtokenx-anchor/Anchor.toml`
pub const REGISTRY_PROGRAM_ID: &str = "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7";
pub const ORACLE_PROGRAM_ID: &str = "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE";
pub const GOVERNANCE_PROGRAM_ID: &str = "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe";
pub const ENERGY_TOKEN_PROGRAM_ID: &str = "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur";
pub const TRADING_PROGRAM_ID: &str = "GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk";

/// Blockchain service for interacting with Solana programs
#[derive(Clone)]
pub struct BlockchainService {
    rpc_client: Arc<RpcClient>,
    cluster: String,
}

impl BlockchainService {
    /// Create a new blockchain service
    pub fn new(rpc_url: String, cluster: String) -> Result<Self> {
        info!("Initializing blockchain service for cluster: {}", cluster);

        let rpc_client = RpcClient::new(rpc_url);

        Ok(Self {
            rpc_client: Arc::new(rpc_client),
            cluster,
        })
    }

    /// Get the RPC client
    pub fn client(&self) -> &RpcClient {
        &self.rpc_client
    }

    /// Get the cluster name
    pub fn cluster(&self) -> &str {
        &self.cluster
    }

    /// Check if the service is healthy by querying the network
    pub async fn health_check(&self) -> Result<bool> {
        let client = self.rpc_client.clone();
        let result = tokio::task::spawn_blocking(move || client.get_health()).await?;

        match result {
            Ok(_) => {
                debug!("Blockchain health check passed");
                Ok(true)
            }
            Err(e) => {
                warn!("Blockchain health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Request airdrop (devnet/localnet only)
    pub async fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        let client = self.rpc_client.clone();
        let pubkey = *pubkey;
        tokio::task::spawn_blocking(move || {
            client
                .request_airdrop(&pubkey, lamports)
                .map_err(|e| anyhow!("Failed to request airdrop: {}", e))
        })
        .await?
    }

    /// Get account balance in lamports
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let client = self.rpc_client.clone();
        let pubkey = *pubkey;
        tokio::task::spawn_blocking(move || {
            client
                .get_balance(&pubkey)
                .map_err(|e| anyhow!("Failed to get balance: {}", e))
        })
        .await?
    }

    /// Get account balance in SOL
    /// Get account balance in SOL
    pub async fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        let lamports = self.get_balance(pubkey).await?;
        Ok(lamports as f64 / 1_000_000_000.0)
    }

    /// Send and confirm a transaction
    /// Send and confirm a transaction
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        let client = self.rpc_client.clone();
        let transaction = transaction.clone();
        tokio::task::spawn_blocking(move || {
            client
                .send_and_confirm_transaction(&transaction)
                .map_err(|e| anyhow!("Failed to send transaction: {}", e))
        })
        .await?
    }

    /// Get transaction status
    /// Get transaction status
    pub async fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>> {
        let client = self.rpc_client.clone();
        let signature = *signature;
        let result =
            tokio::task::spawn_blocking(move || client.get_signature_status(&signature)).await?;

        match result {
            Ok(status) => Ok(status.map(|s| s.is_ok())),
            Err(e) => Err(anyhow!("Failed to get signature status: {}", e)),
        }
    }

    /// Get recent blockhash
    /// Get recent blockhash
    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        let client = self.rpc_client.clone();
        tokio::task::spawn_blocking(move || {
            client
                .get_latest_blockhash()
                .map_err(|e| anyhow!("Failed to get latest blockhash: {}", e))
        })
        .await?
    }

    /// Get slot height
    /// Get slot height
    pub async fn get_slot(&self) -> Result<u64> {
        let client = self.rpc_client.clone();
        tokio::task::spawn_blocking(move || {
            client
                .get_slot()
                .map_err(|e| anyhow!("Failed to get slot: {}", e))
        })
        .await?
    }

    /// Get account data
    /// Get account data
    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let client = self.rpc_client.clone();
        let pubkey = *pubkey;
        let account = tokio::task::spawn_blocking(move || {
            client
                .get_account(&pubkey)
                .map_err(|e| anyhow!("Failed to get account: {}", e))
        })
        .await??;
        Ok(account.data)
    }

    /// Check if an account exists
    /// Check if an account exists
    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        let client = self.rpc_client.clone();
        let pubkey = *pubkey;
        let result = tokio::task::spawn_blocking(move || client.get_account(&pubkey)).await?;

        match result {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Parse Pubkey from string
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        Pubkey::from_str(pubkey_str)
            .map_err(|e| anyhow!("Invalid public key '{}': {}", pubkey_str, e))
    }

    /// Get Registry program ID
    pub fn registry_program_id() -> Result<Pubkey> {
        Self::parse_pubkey(REGISTRY_PROGRAM_ID)
    }

    /// Get Oracle program ID
    pub fn oracle_program_id() -> Result<Pubkey> {
        Self::parse_pubkey(ORACLE_PROGRAM_ID)
    }

    /// Get Governance program ID
    pub fn governance_program_id() -> Result<Pubkey> {
        Self::parse_pubkey(GOVERNANCE_PROGRAM_ID)
    }

    /// Get Energy Token program ID
    pub fn energy_token_program_id() -> Result<Pubkey> {
        Self::parse_pubkey(ENERGY_TOKEN_PROGRAM_ID)
    }

    /// Get Trading program ID
    pub fn trading_program_id() -> Result<Pubkey> {
        Self::parse_pubkey(TRADING_PROGRAM_ID)
    }

    // ====================================================================
    // Transaction Building & Signing (Phase 4)
    // ====================================================================

    /// Priority 4: Build, sign, and send a transaction with automatic priority fees
    /// Returns transaction signature with enhanced performance monitoring
    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        let start_time = std::time::Instant::now();

        let result = self
            .build_and_send_transaction_with_priority(
                instructions,
                signers,
                TransactionType::OrderCreation,
            )
            .await;

        let duration = start_time.elapsed();
        info!(
            "Priority 4: Transaction build & send completed in {:?}",
            duration
        );

        result
    }

    /// Build, sign, and send a transaction with specified priority level
    /// Returns transaction signature
    pub async fn build_and_send_transaction_with_priority(
        &self,
        mut instructions: Vec<Instruction>,
        signers: &[&Keypair],
        transaction_type: TransactionType,
    ) -> Result<Signature> {
        // Add priority fees based on transaction type
        let priority_level = PriorityFeeService::recommend_priority_level(transaction_type);
        let compute_limit = PriorityFeeService::recommend_compute_limit(transaction_type);

        PriorityFeeService::add_priority_fee(
            &mut instructions,
            priority_level,
            Some(compute_limit),
        )?;

        // Get recent blockhash
        let recent_blockhash = self.get_latest_blockhash().await?;

        // Determine payer (first signer)
        let payer_keypair = *signers
            .first()
            .ok_or_else(|| anyhow!("At least one signer required"))?;
        let payer = payer_keypair.pubkey();

        // Build transaction
        let mut transaction = Transaction::new_with_payer(&instructions, Some(&payer));

        // Sign transaction
        transaction
            .try_sign(signers, recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

        // Send transaction
        let signature = self.send_and_confirm_transaction(&transaction).await?;

        let estimated_cost =
            PriorityFeeService::estimate_fee_cost(priority_level, Some(compute_limit));
        info!(
            "Transaction sent successfully: {} (priority: {}, estimated cost: {} SOL)",
            signature,
            priority_level.description(),
            estimated_cost
        );
        Ok(signature)
    }

    /// Simulate a transaction before sending
    /// Returns whether the simulation succeeded
    /// Simulate a transaction before sending
    /// Returns whether the simulation succeeded
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<bool> {
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            ..Default::default()
        };

        let client = self.rpc_client.clone();
        let transaction = transaction.clone();

        let result = tokio::task::spawn_blocking(move || {
            client.simulate_transaction_with_config(&transaction, config)
        })
        .await?;

        match result {
            Ok(response) => {
                if let Some(err) = response.value.err {
                    warn!("Transaction simulation failed: {:?}", err);
                    Ok(false)
                } else {
                    debug!("Transaction simulation succeeded");
                    Ok(true)
                }
            }
            Err(e) => {
                error!("Failed to simulate transaction: {}", e);
                Err(anyhow!("Transaction simulation error: {}", e))
            }
        }
    }

    /// Wait for transaction confirmation with timeout
    /// Returns true if confirmed, false if timeout
    pub async fn wait_for_confirmation(
        &self,
        signature: &Signature,
        timeout_secs: u64,
    ) -> Result<bool> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                warn!(
                    "Transaction confirmation timeout after {}s: {}",
                    timeout_secs, signature
                );
                return Ok(false);
            }

            match self.get_signature_status(signature).await? {
                Some(true) => {
                    info!("Transaction confirmed: {}", signature);
                    return Ok(true);
                }
                Some(false) => {
                    error!("Transaction failed: {}", signature);
                    return Err(anyhow!("Transaction failed on-chain"));
                }
                None => {
                    debug!("Transaction not yet confirmed: {}", signature);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// Send transaction with retry logic
    /// Retries up to max_retries times on failure
    pub async fn send_transaction_with_retry(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
        max_retries: u32,
    ) -> Result<Signature> {
        let mut last_error = None;

        for attempt in 1..=max_retries {
            debug!("Transaction attempt {}/{}", attempt, max_retries);

            match self
                .build_and_send_transaction(instructions.clone(), signers)
                .await
            {
                Ok(signature) => {
                    // Wait for confirmation
                    match self.wait_for_confirmation(&signature, 30).await {
                        Ok(true) => return Ok(signature),
                        Ok(false) => {
                            last_error = Some(anyhow!("Transaction confirmation timeout"));
                        }
                        Err(e) => {
                            last_error = Some(e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Transaction attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                }
            }

            if attempt < max_retries {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow!("Transaction failed after {} retries", max_retries)))
    }

    /// Build a transaction without sending
    /// Useful for inspection or simulation
    /// Build a transaction without sending
    /// Useful for inspection or simulation
    pub async fn build_transaction(
        &self,
        instructions: Vec<Instruction>,
        payer: &Pubkey,
    ) -> Result<Transaction> {
        let recent_blockhash = self.get_latest_blockhash().await?;
        let mut transaction = Transaction::new_with_payer(&instructions, Some(payer));
        transaction.message.recent_blockhash = recent_blockhash;
        Ok(transaction)
    }

    /// Mint energy tokens directly to a user's token account
    /// This calls the energy_token program's mint_tokens_direct instruction
    pub async fn mint_energy_tokens(
        &self,
        authority: &Keypair,
        user_token_account: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        info!(
            "Minting {} kWh as tokens to {}",
            amount_kwh, user_token_account
        );

        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;

        // Derive token_info PDA
        let energy_token_program = Self::energy_token_program_id()?;
        let (token_info_pda, _bump) =
            Pubkey::find_program_address(&[b"token_info"], &energy_token_program);

        debug!("Token info PDA: {}", token_info_pda);
        debug!("Mint: {}", mint);
        debug!("Amount (lamports): {}", amount_lamports);

        // Build the instruction data: discriminator (8 bytes) + amount (8 bytes)
        // For Anchor, the discriminator is the first 8 bytes of sha256("global:mint_tokens_direct")
        let mut instruction_data = Vec::with_capacity(16);

        // Calculate Anchor discriminator for "mint_tokens_direct"
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:mint_tokens_direct");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);

        // Add amount as u64 (little-endian)
        instruction_data.extend_from_slice(&amount_lamports.to_le_bytes());

        // Token program ID
        let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
            .expect("Valid token program ID");

        // Build accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(token_info_pda, false),
            solana_sdk::instruction::AccountMeta::new(*mint, false),
            solana_sdk::instruction::AccountMeta::new(*user_token_account, false),
            solana_sdk::instruction::AccountMeta::new_readonly(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(token_program_id, false),
        ];

        let mint_instruction =
            Instruction::new_with_bytes(energy_token_program, &instruction_data, accounts);

        // Build and send transaction with token minting priority
        let signers = vec![authority];
        let signature = self
            .build_and_send_transaction_with_priority(
                vec![mint_instruction],
                &signers,
                TransactionType::TokenMinting,
            )
            .await?;

        info!(
            "Successfully minted {} kWh as tokens. Signature: {}",
            amount_kwh, signature
        );

        Ok(signature)
    }

    /// Ensures user has an Associated Token Account for the token mint
    /// Creates ATA if it doesn't exist, returns ATA address
    pub async fn ensure_token_account_exists(
        &self,
        authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Pubkey> {
        // Calculate ATA address manually to avoid type conversion issues
        // ATA = PDA of [associated_token_account_program_id, wallet, token_program_id, mint]
        let ata_program_id = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .map_err(|e| anyhow!("Invalid ATA program ID: {}", e))?;

        let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
            .map_err(|e| anyhow!("Invalid token program ID: {}", e))?;

        let (ata_address, _bump) = Pubkey::find_program_address(
            &[
                user_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            &ata_program_id,
        );

        // Check if account exists
        if self.account_exists(&ata_address).await? {
            info!("ATA already exists: {}", ata_address);
            return Ok(ata_address);
        }

        info!("Creating ATA for user: {}", user_wallet);

        // ATA creation instruction data (empty for associated token account creation)
        let instruction_data = vec![];

        // Accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(ata_address, false), // ATA account (writable)
            solana_sdk::instruction::AccountMeta::new(*user_wallet, false), // Wallet owner
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true), // Payer (signer)
            solana_sdk::instruction::AccountMeta::new(*mint, false),       // Mint
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ), // System program
            solana_sdk::instruction::AccountMeta::new_readonly(token_program_id, false), // Token program
            solana_sdk::instruction::AccountMeta::new_readonly(ata_program_id, false), // ATA program
        ];

        let create_ata_ix = Instruction {
            program_id: ata_program_id,
            accounts,
            data: instruction_data,
        };

        // Submit transaction
        let signature = self
            .build_and_send_transaction(vec![create_ata_ix], &[authority])
            .await?;

        info!("ATA created. Signature: {}", signature);

        // Wait for confirmation
        self.wait_for_confirmation(&signature, 30).await?;

        Ok(ata_address)
    }

    /// Transfer SPL tokens from one account to another
    /// Used for settlement transfers: buyer → seller
    pub async fn transfer_tokens(
        &self,
        authority: &Keypair,
        from_token_account: &Pubkey,
        to_token_account: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        info!(
            "Transferring {} tokens from {} to {}",
            amount, from_token_account, to_token_account
        );

        // Create transfer instruction manually to avoid type conflicts
        let token_program_id = solana_sdk::pubkey::Pubkey::from(spl_token::id().to_bytes());

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

        let transfer_instruction = solana_sdk::instruction::Instruction {
            program_id: token_program_id,
            accounts,
            data: instruction_data,
        };

        // Submit transaction with settlement priority
        let signature = self
            .build_and_send_transaction_with_priority(
                vec![transfer_instruction],
                &[authority],
                TransactionType::Settlement,
            )
            .await?;

        info!("Tokens transferred. Signature: {}", signature);

        // Wait for confirmation
        self.wait_for_confirmation(&signature, 30).await?;

        Ok(signature)
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

    /// Get authority keypair (for settlement service)
    /// Loads the keypair from dev-wallet.json in the project root
    pub async fn get_authority_keypair(&self) -> Result<Keypair> {
        // In production, this should use secure key management (e.g., AWS KMS, HashiCorp Vault)
        // For development, we load from a local file

        // Try to load from dev-wallet.json in the project root
        let wallet_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "dev-wallet.json".to_string());

        info!("Loading authority keypair from: {}", wallet_path);

        Self::load_keypair_from_file(&wallet_path)
    }

    /// Register a user on-chain
    pub async fn register_user_on_chain(
        &self,
        authority: &Keypair,
        user_type: u8, // 0: Prosumer, 1: Consumer
        location: &str,
    ) -> Result<Signature> {
        info!("Registering user on-chain: {}", authority.pubkey());

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

        let instruction =
            Instruction::new_with_bytes(registry_program_id, &instruction_data, accounts);

        self.build_and_send_transaction(vec![instruction], &[authority])
            .await
    }

    /// Register a meter on-chain
    pub async fn register_meter_on_chain(
        &self,
        authority: &Keypair,
        meter_id: &str,
        meter_type: u8, // 0: Solar, 1: Wind, etc.
    ) -> Result<Signature> {
        info!("Registering meter on-chain: {}", meter_id);

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

        let instruction =
            Instruction::new_with_bytes(registry_program_id, &instruction_data, accounts);

        self.build_and_send_transaction(vec![instruction], &[authority])
            .await
    }

    /// Submit meter reading on-chain (via Oracle)
    pub async fn submit_meter_reading_on_chain(
        &self,
        authority: &Keypair, // Must be API Gateway authority
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Signature> {
        info!("Submitting meter reading on-chain: {}", meter_id);

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

        let instruction =
            Instruction::new_with_bytes(oracle_program_id, &instruction_data, accounts);

        self.build_and_send_transaction(vec![instruction], &[authority])
            .await
    }
}

/// Helper functions for transaction building
pub mod transaction_utils {
    use super::*;
    use solana_sdk::instruction::Instruction;

    /// Build a transaction from instructions
    pub fn build_transaction(
        instructions: Vec<Instruction>,
        payer: &Pubkey,
        _recent_blockhash: solana_sdk::hash::Hash,
    ) -> Transaction {
        Transaction::new_with_payer(&instructions, Some(payer))
    }

    /// Sign a transaction
    pub fn sign_transaction(
        transaction: &mut Transaction,
        signers: &[&Keypair],
        recent_blockhash: solana_sdk::hash::Hash,
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

    // TODO: Fix this function - it's outside of impl block
    // pub async fn mint_energy_tokens_batch(
    //     &self,
    //     authority: &Keypair,
    //     mint: &Pubkey,
    //     batch_data: &[MintBatchData],
    //     max_tx_per_batch: usize,
    // ) -> Result<Vec<MintBatchResult>> {
    //     // Implementation moved outside of impl block
    // }

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
        let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
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

    // TODO: Fix this function - it's outside of impl block
    // pub async fn mint_tokens_direct(&self, user_wallet: &Pubkey, amount: u64) -> Result<Signature> {
    //     // Get authority keypair
    //     let authority = self.get_authority_keypair().await?;
    //
    //     // Get energy token mint
    //     let mint = Pubkey::from_str(
    //         &std::env::var("ENERGY_TOKEN_MINT")
    //             .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?,
    //     )?;
    //
    //     // Ensure user has an associated token account
    //     let user_token_account = self
    //         .ensure_token_account_exists(&authority, user_wallet, &mint)
    //         .await?;
    //
    //     // Call the original mint method
    //     self.mint_energy_tokens(
    //         &authority,
    //         &user_token_account,
    //         &mint,
    //         amount as f64 / 1_000_000_000.0,
    //     )
    //     .await
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_program_ids() {
        assert!(BlockchainService::registry_program_id().is_ok());
        assert!(BlockchainService::oracle_program_id().is_ok());
        assert!(BlockchainService::governance_program_id().is_ok());
        assert!(BlockchainService::energy_token_program_id().is_ok());
        assert!(BlockchainService::trading_program_id().is_ok());
    }

    #[test]
    fn test_parse_invalid_pubkey() {
        assert!(BlockchainService::parse_pubkey("invalid").is_err());
    }
}
