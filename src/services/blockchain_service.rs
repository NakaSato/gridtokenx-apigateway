use anyhow::{anyhow, Result};
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
use tracing::{debug, info, warn, error};

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
        match self.rpc_client.get_health() {
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

    /// Get account balance in lamports
    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.rpc_client
            .get_balance(pubkey)
            .map_err(|e| anyhow!("Failed to get balance: {}", e))
    }

    /// Get account balance in SOL
    pub fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        let lamports = self.get_balance(pubkey)?;
        Ok(lamports as f64 / 1_000_000_000.0)
    }

    /// Send and confirm a transaction
    pub fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        self.rpc_client
            .send_and_confirm_transaction(transaction)
            .map_err(|e| anyhow!("Failed to send transaction: {}", e))
    }

    /// Get transaction status
    pub fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>> {
        match self.rpc_client.get_signature_status(signature) {
            Ok(status) => Ok(status.map(|s| s.is_ok())),
            Err(e) => Err(anyhow!("Failed to get signature status: {}", e)),
        }
    }

    /// Get recent blockhash
    pub fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.rpc_client
            .get_latest_blockhash()
            .map_err(|e| anyhow!("Failed to get latest blockhash: {}", e))
    }

    /// Get slot height
    pub fn get_slot(&self) -> Result<u64> {
        self.rpc_client
            .get_slot()
            .map_err(|e| anyhow!("Failed to get slot: {}", e))
    }

    /// Get account data
    pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self.rpc_client
            .get_account(pubkey)
            .map_err(|e| anyhow!("Failed to get account: {}", e))?;
        Ok(account.data)
    }

    /// Check if an account exists
    pub fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.rpc_client.get_account(pubkey) {
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

    /// Build, sign, and send a transaction
    /// Returns the transaction signature
    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        // Get recent blockhash
        let recent_blockhash = self.get_latest_blockhash()?;
        
        // Determine payer (first signer)
        let payer_keypair = *signers.first()
            .ok_or_else(|| anyhow!("At least one signer required"))?;
        let payer = payer_keypair.pubkey();

        // Build transaction
        let mut transaction = Transaction::new_with_payer(&instructions, Some(&payer));
        
        // Sign transaction
        transaction.try_sign(signers, recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

        // Send transaction
        let signature = self.send_and_confirm_transaction(&transaction)?;
        
        info!("Transaction sent successfully: {}", signature);
        Ok(signature)
    }

    /// Simulate a transaction before sending
    /// Returns whether the simulation succeeded
    pub fn simulate_transaction(&self, transaction: &Transaction) -> Result<bool> {
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            ..Default::default()
        };

        match self.rpc_client.simulate_transaction_with_config(transaction, config) {
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
                warn!("Transaction confirmation timeout after {}s: {}", timeout_secs, signature);
                return Ok(false);
            }

            match self.get_signature_status(signature)? {
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

            match self.build_and_send_transaction(instructions.clone(), signers).await {
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

        Err(last_error.unwrap_or_else(|| anyhow!("Transaction failed after {} retries", max_retries)))
    }

    /// Build a transaction without sending
    /// Useful for inspection or simulation
    pub fn build_transaction(
        &self,
        instructions: Vec<Instruction>,
        payer: &Pubkey,
    ) -> Result<Transaction> {
        let recent_blockhash = self.get_latest_blockhash()?;
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
        info!("Minting {} kWh as tokens to {}", amount_kwh, user_token_account);
        
        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;
        
        // Derive token_info PDA
        let energy_token_program = Self::energy_token_program_id()?;
        let (token_info_pda, _bump) = Pubkey::find_program_address(
            &[b"token_info"],
            &energy_token_program,
        );
        
        debug!("Token info PDA: {}", token_info_pda);
        debug!("Mint: {}", mint);
        debug!("Amount (lamports): {}", amount_lamports);
        
        // Build the instruction data: discriminator (8 bytes) + amount (8 bytes)
        // For Anchor, the discriminator is the first 8 bytes of sha256("global:mint_tokens_direct")
        let mut instruction_data = Vec::with_capacity(16);
        
        // Calculate Anchor discriminator for "mint_tokens_direct"
        use sha2::{Sha256, Digest};
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
        
        let mint_instruction = Instruction::new_with_bytes(
            energy_token_program,
            &instruction_data,
            accounts,
        );
        
        // Build and send transaction
        let signers = vec![authority];
        let signature = self.build_and_send_transaction(
            vec![mint_instruction],
            &signers,
        ).await?;
        
        info!("Successfully minted {} kWh as tokens. Signature: {}", amount_kwh, signature);
        
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
        if self.account_exists(&ata_address)? {
            info!("ATA already exists: {}", ata_address);
            return Ok(ata_address);
        }
        
        info!("Creating ATA for user: {}", user_wallet);
        
        // ATA creation instruction data (empty for associated token account creation)
        let instruction_data = vec![];
        
        // Accounts for the instruction
        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(ata_address, false),     // ATA account (writable)
            solana_sdk::instruction::AccountMeta::new(*user_wallet, false),      // Wallet owner
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true), // Payer (signer)
            solana_sdk::instruction::AccountMeta::new(*mint, false),            // Mint
            solana_sdk::instruction::AccountMeta::new_readonly(
                Pubkey::from_str("11111111111111111111111111111112")
                    .expect("Valid system program ID"), false
            ), // System program
            solana_sdk::instruction::AccountMeta::new_readonly(
                token_program_id, false
            ), // Token program
            solana_sdk::instruction::AccountMeta::new_readonly(
                ata_program_id, false
            ), // ATA program
        ];
        
        let create_ata_ix = Instruction {
            program_id: ata_program_id,
            accounts,
            data: instruction_data,
        };
        
        // Submit transaction
        let signature = self.build_and_send_transaction(
            vec![create_ata_ix],
            &[authority],
        ).await?;
        
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
        
        // Create transfer instruction using transfer_checked for safety
        let transfer_ix = spl_token::instruction::transfer_checked(
            &spl_token::id(),
            from_token_account,
            mint,
            to_token_account,
            &authority.pubkey(),  // Authority (owner of from_account)
            &[],                   // No multisig signers
            amount,
            decimals,
        )?;
        
        // Submit transaction
        let signature = self.build_and_send_transaction(
            vec![transfer_ix],
            &[authority],
        ).await?;
        
        info!("Tokens transferred. Signature: {}", signature);
        
        // Wait for confirmation
        self.wait_for_confirmation(&signature, 30).await?;
        
        Ok(signature)
    }

    /// Get authority keypair (for settlement service)
    /// This should be implemented by the wallet service, but providing a placeholder
    pub async fn get_authority_keypair(&self) -> Result<Keypair> {
        // This should be moved to wallet service in production
        // For now, create a test keypair - in production this should load from secure storage
        warn!("Using placeholder authority keypair - implement proper wallet service integration");
        Err(anyhow!("Authority keypair not implemented - integrate with wallet service"))
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
        transaction.try_sign(signers, recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;
        Ok(())
    }
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
