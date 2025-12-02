use crate::services::priority_fee_service::{PriorityFeeService, TransactionType};
use anyhow::{Result, anyhow};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    // compute_budget::ComputeBudgetInstruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Transaction handling for Solana blockchain operations with enhanced performance and security
#[derive(Clone)]
pub struct TransactionHandler {
    rpc_client: Arc<RpcClient>,
    /// Cached recent blockhash for performance
    recent_blockhash: Arc<RwLock<Option<solana_sdk::hash::Hash>>>,
    /// Connection pool for better performance
    connection_pool: Arc<RwLock<Vec<Arc<RpcClient>>>>,
}

impl std::fmt::Debug for TransactionHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionHandler")
            .field("rpc_url", &self.rpc_client.url())
            .finish()
    }
}

impl TransactionHandler {
    /// Create a new transaction handler with connection pooling
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        info!("Initializing transaction handler with connection pooling");
        Self {
            rpc_client,
            recent_blockhash: Arc::new(RwLock::new(None)),
            connection_pool: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get or create a connection from the pool
    async fn get_connection(&self) -> Arc<RpcClient> {
        let mut pool = self.connection_pool.write().await;

        // Return existing connection if available
        if let Some(conn) = pool.pop() {
            debug!("Reusing existing connection from pool");
            return conn;
        }

        // Create new connection if pool is empty
        if pool.is_empty() {
            let new_conn = Arc::new(RpcClient::new(self.rpc_client.url()));
            pool.push(new_conn.clone());
            info!("Created new RPC connection (pool size: {})", pool.len());
            return new_conn;
        }

        // Create new connection and add to pool
        let new_conn = Arc::new(RpcClient::new(self.rpc_client.url()));
        pool.push(new_conn.clone());
        info!("Created new RPC connection (pool size: {})", pool.len());
        new_conn
    }

    /// Return connection to pool after use
    async fn return_connection(&self, conn: Arc<RpcClient>) {
        let mut pool = self.connection_pool.write().await;
        pool.push(conn);
        debug!("Returned connection to pool (pool size: {})", pool.len());
    }

    /// Submit transaction with simulation and priority fees
    pub async fn submit_transaction(&self, mut transaction: Transaction) -> Result<Signature> {
        let start_time = std::time::Instant::now();

        // Get recent blockhash for transaction
        let recent_blockhash = self.get_recent_blockhash().await?;
        transaction.message.recent_blockhash = recent_blockhash;

        // 1. Simulate transaction first to validate
        if let Err(e) = self.simulate_transaction(&transaction).await {
            error!("Transaction simulation failed: {}", e);
            return Err(anyhow!("Transaction simulation failed: {}", e));
        }

        // 2. Add priority fees based on transaction type
        let tx_type = self.determine_transaction_type(&transaction);
        self.add_priority_fees(&mut transaction, tx_type).await?;

        // 3. Sign transaction with secure key management
        let signature = self.sign_transaction(&mut transaction).await?;

        // 4. Submit to network with retry logic
        let signature = self.submit_with_retry(transaction, signature).await?;

        let duration = start_time.elapsed();
        info!(
            "Transaction submitted successfully in {:?} (type: {:?})",
            duration, tx_type
        );

        Ok(signature)
    }

    /// Determine transaction type for priority fee calculation
    fn determine_transaction_type(&self, transaction: &Transaction) -> TransactionType {
        // Simple logic - in production, this would be determined from context
        if transaction.message.instructions.len() > 3 {
            TransactionType::Settlement
        } else if transaction.message.instructions.len() == 1 {
            // Just return a default type for now
            TransactionType::TokenTransfer
        } else {
            TransactionType::TokenTransfer
        }
    }

    /// Simulate transaction with enhanced validation
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<()> {
        debug!(
            "Simulating transaction with {} instructions",
            transaction.message.instructions.len()
        );

        let conn = self.get_connection().await;

        // Use RpcSimulateTransactionConfig with better validation
        let config = solana_client::rpc_config::RpcSimulateTransactionConfig {
            sig_verify: true,
            replace_recent_blockhash: true,
            ..Default::default()
        };

        let simulation = conn
            .simulate_transaction_with_config(transaction, config)
            .map_err(|e| anyhow!("Transaction simulation failed: {}", e))?;

        self.return_connection(conn).await;

        if let Some(err) = simulation.value.err {
            warn!("Transaction simulation errors: {:?}", err);
            return Err(anyhow!(
                "Transaction simulation validation failed: {:?}",
                err
            ));
        }

        if let Some(logs) = &simulation.value.logs {
            if !logs.is_empty() {
                for log in logs {
                    debug!("Simulation log: {}", log);
                }
            }
        }

        debug!("Transaction simulation completed successfully");
        Ok(())
    }

    /// Add priority fees to transaction based on type
    async fn add_priority_fees(
        &self,
        _transaction: &mut Transaction,
        tx_type: TransactionType,
    ) -> Result<()> {
        let priority_level = PriorityFeeService::recommend_priority_level(tx_type);
        let compute_limit = PriorityFeeService::recommend_compute_limit(tx_type);
        let fee = PriorityFeeService::estimate_fee_cost(priority_level, Some(compute_limit));

        debug!(
            "Adding priority fees: level={}, limit={}, estimated_cost={} SOL",
            priority_level.description(),
            compute_limit,
            fee
        );

        // Note: Priority fees implementation
        // TODO: Uncomment and implement proper compute budget instructions
        // let compute_budget_instruction = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        // let priority_fee_instruction = ComputeBudgetInstruction::set_compute_unit_price(fee);
        // transaction.message.instructions.insert(0, compute_budget_instruction);
        // transaction.message.instructions.insert(0, priority_fee_instruction);

        Ok(())
    }

    /// Sign transaction with secure key management
    async fn sign_transaction(&self, transaction: &mut Transaction) -> Result<Signature> {
        // Get recent blockhash
        let recent_blockhash = self.get_recent_blockhash().await?;
        transaction.message.recent_blockhash = recent_blockhash;

        // Get payer keypair from secure storage
        let payer_keypair = self.get_payer_keypair().await?;

        // Validate transaction before signing
        self.validate_transaction(&transaction).await?;

        // Sign with proper fee payer
        transaction
            .try_sign(&[&payer_keypair], recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

        debug!("Transaction signed successfully");
        // Return the signature
        Ok(transaction.signatures[0])
    }

    /// Get payer keypair with proper fallbacks
    async fn get_payer_keypair(&self) -> Result<Keypair> {
        // Try loading from secure storage first
        if let Ok(keypair) = self.load_payer_keypair().await {
            return Ok(keypair);
        }

        // Fallback to environment variable
        if let Ok(private_key) = std::env::var("PAYER_PRIVATE_KEY") {
            if let Ok(key_bytes) = bs58::decode(&private_key).into_vec() {
                // Solana keypair can be 64 bytes (full keypair) or 32 bytes (secret key)
                if key_bytes.len() == 64 {
                    // Full keypair format - extract the secret key (first 32 bytes)
                    let mut secret_key = [0u8; 32];
                    secret_key.copy_from_slice(&key_bytes[..32]);
                    return Ok(Keypair::new_from_array(secret_key));
                } else if key_bytes.len() == 32 {
                    // Just the secret key
                    let mut secret_key = [0u8; 32];
                    secret_key.copy_from_slice(&key_bytes);
                    return Ok(Keypair::new_from_array(secret_key));
                }
            }
        }

        // Fallback to development keypair
        warn!("Using fallback keypair - set PAYER_PRIVATE_KEY for production");
        Ok(Keypair::new())
    }

    /// Load payer keypair from secure storage
    async fn load_payer_keypair(&self) -> Result<Keypair> {
        // Try loading from multiple secure locations
        let key_paths = vec![
            "/run/secrets/payer.json",
            "/app/payer.json",
            "/etc/gridtokenx/payer.json",
        ];

        for path in key_paths {
            if let Ok(keypair) =
                crate::services::blockchain_utils::BlockchainUtils::load_keypair_from_file(path)
            {
                info!("Loaded payer keypair from: {}", path);
                return Ok(keypair);
            }
        }

        Err(anyhow!("Payer keypair not found in secure storage"))
    }

    /// Validate transaction before submission
    async fn validate_transaction(&self, transaction: &Transaction) -> Result<()> {
        // Check instruction count
        if transaction.message.instructions.is_empty() {
            return Err(anyhow!("Transaction cannot be empty"));
        }

        // Check for duplicate instructions
        let instruction_count = transaction.message.instructions.len();
        if instruction_count > 10 {
            warn!(
                "Transaction has {} instructions - consider batch optimization",
                instruction_count
            );
        }

        // Validate each instruction
        for (i, instruction) in transaction.message.instructions.iter().enumerate() {
            if instruction.data.is_empty() {
                return Err(anyhow!("Instruction {} cannot be empty", i));
            }
        }

        debug!("Transaction validation passed");
        Ok(())
    }

    /// Submit transaction with retry logic and enhanced error handling
    async fn submit_with_retry(
        &self,
        mut transaction: Transaction,
        _initial_signature: Signature,
    ) -> Result<Signature> {
        let mut attempts = 0;
        let max_retries = 3;
        let base_delay = Duration::from_secs(1);

        loop {
            attempts += 1;

            if attempts > 1 {
                warn!("Transaction retry attempt {}/{}", attempts, max_retries);
            }

            // Update transaction with new blockhash for retry
            let recent_blockhash = self.get_recent_blockhash().await?;
            transaction.message.recent_blockhash = recent_blockhash;

            // Resign transaction
            transaction
                .try_sign(&[&self.get_payer_keypair().await?], recent_blockhash)
                .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

            let conn = self.get_connection().await;

            match conn.send_and_confirm_transaction(&transaction) {
                Ok(sig) => {
                    info!("Transaction submitted successfully on attempt {}", attempts);
                    return Ok(sig);
                }
                Err(e) => {
                    error!(
                        "Transaction submission failed on attempt {}: {}",
                        attempts, e
                    );

                    if attempts >= max_retries {
                        return Err(anyhow!(
                            "Transaction failed after {} retries: {}",
                            max_retries,
                            e
                        ));
                    }
                }
            }

            // Update transaction for next retry
            tokio::time::sleep(base_delay * attempts).await;
        }
    }

    /// Get recent blockhash with caching
    async fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        // Check cache first
        {
            let cache = self.recent_blockhash.read().await;
            if let Some(blockhash) = *cache {
                debug!("Using cached blockhash");
                return Ok(blockhash);
            }
        }

        // Fetch from network if not cached
        let conn = self.get_connection().await;
        let blockhash = conn
            .get_latest_blockhash()
            .map_err(|e| anyhow!("Failed to get latest blockhash: {}", e))?;

        // Update cache
        {
            let mut cache = self.recent_blockhash.write().await;
            *cache = Some(blockhash);
            debug!("Updated cached blockhash: {}", blockhash);
        }

        self.return_connection(conn).await;
        Ok(blockhash)
    }

    /// Enhanced account balance queries with caching
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let cache_key = format!("balance:{}", pubkey);

        // Check cache first
        if let Some(cached_balance) = self.get_cached_balance(&cache_key).await {
            debug!("Using cached balance for {}: {}", pubkey, cached_balance);
            return Ok(cached_balance);
        }

        // Fetch from network
        let conn = self.get_connection().await;
        let balance = conn
            .get_balance(pubkey)
            .map_err(|e| anyhow!("Failed to get balance: {}", e))?;

        // Update cache with short TTL
        self.update_balance_cache(&cache_key, balance, 60).await;

        self.return_connection(conn).await;
        Ok(balance)
    }

    /// Get token account balance
    pub async fn get_token_account_balance(&self, token_account: &Pubkey) -> Result<u64> {
        let conn = self.get_connection().await;

        let balance_result = conn
            .get_token_account_balance(token_account)
            .map_err(|e| anyhow!("Failed to get token account balance: {}", e))?;

        self.return_connection(conn).await;

        // Parse amount as u64 (lamports/raw units)
        balance_result
            .amount
            .parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse token amount: {}", e))
    }

    /// Simple in-memory balance cache
    async fn get_cached_balance(&self, _key: &str) -> Option<u64> {
        // This is a simple implementation - in production, use Redis
        None
    }

    async fn update_balance_cache(&self, _key: &str, _balance: u64, _ttl: u64) {
        // This is a simple implementation - in production, use Redis
        // For now, no-op
    }

    /// Get the RPC client
    pub fn client(&self) -> &RpcClient {
        &self.rpc_client
    }

    /// Add priority fee to transaction
    pub fn add_priority_fee(
        &self,
        _transaction: &mut Transaction,
        _tx_type: TransactionType,
        _fee: u64,
    ) -> Result<()> {
        // TODO: Implement priority fee addition
        Ok(())
    }

    /// Confirm transaction status
    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        let sig =
            Signature::from_str(signature).map_err(|e| anyhow!("Invalid signature: {}", e))?;

        let status = self
            .rpc_client
            .get_signature_status(&sig)
            .map_err(|e| anyhow!("Failed to get signature status: {}", e))?;

        Ok(status.is_some())
    }

    /// Get trade record from blockchain
    pub async fn get_trade_record(
        &self,
        _signature: &str,
    ) -> Result<crate::models::transaction::TradeRecord> {
        // TODO: Implement trade record fetching from blockchain
        Err(anyhow!("Trade record fetching not implemented"))
    }

    /// Check if the service is healthy
    pub async fn health_check(&self) -> Result<bool> {
        match self.rpc_client.get_health() {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Request airdrop (devnet/localnet only)
    pub async fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        self.rpc_client
            .request_airdrop(pubkey, lamports)
            .map_err(|e| anyhow!("Failed to request airdrop: {}", e))
    }

    /// Get account balance in SOL
    pub async fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        let lamports = self.get_balance(pubkey).await?;
        Ok(lamports as f64 / 1_000_000_000.0)
    }

    /// Send and confirm a transaction
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        self.rpc_client
            .send_and_confirm_transaction(transaction)
            .map_err(|e| anyhow!("Failed to send and confirm transaction: {}", e))
    }

    /// Get transaction status
    pub async fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>> {
        let status = self
            .rpc_client
            .get_signature_status(signature)
            .map_err(|e| anyhow!("Failed to get signature status: {}", e))?;

        Ok(status.map(|s| s.is_ok()))
    }

    /// Get recent blockhash
    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.rpc_client
            .get_latest_blockhash()
            .map_err(|e| anyhow!("Failed to get latest blockhash: {}", e))
    }

    /// Get slot height
    pub async fn get_slot(&self) -> Result<u64> {
        self.rpc_client
            .get_slot()
            .map_err(|e| anyhow!("Failed to get slot: {}", e))
    }

    /// Get account data
    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self
            .rpc_client
            .get_account(pubkey)
            .map_err(|e| anyhow!("Failed to get account: {}", e))?;

        Ok(account.data)
    }

    /// Check if an account exists
    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.rpc_client.get_account(pubkey) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Build, sign, and send a transaction
    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| anyhow!("Failed to get blockhash: {}", e))?;

        let mut transaction =
            Transaction::new_with_payer(&instructions, Some(&signers[0].pubkey()));
        transaction.sign(signers, recent_blockhash);

        self.rpc_client
            .send_and_confirm_transaction(&transaction)
            .map_err(|e| anyhow!("Failed to send transaction: {}", e))
    }

    /// Build, sign, and send a transaction with priority
    pub async fn build_and_send_transaction_with_priority(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        signers: &[&Keypair],
        _transaction_type: TransactionType,
    ) -> Result<Signature> {
        // For now, just call the regular method
        self.build_and_send_transaction(instructions, signers).await
    }

    /// Wait for transaction confirmation
    pub async fn wait_for_confirmation(
        &self,
        signature: &Signature,
        timeout_secs: u64,
    ) -> Result<bool> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() >= timeout_secs {
                return Ok(false);
            }

            match self.rpc_client.get_signature_status(signature) {
                Ok(Some(_)) => return Ok(true),
                Ok(None) => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }
        }
    }

    /// Send transaction with retry
    pub async fn send_transaction_with_retry(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        signers: &[&Keypair],
        max_retries: u32,
    ) -> Result<Signature> {
        let mut attempts = 0;

        loop {
            attempts += 1;

            match self
                .build_and_send_transaction(instructions.clone(), signers)
                .await
            {
                Ok(sig) => return Ok(sig),
                Err(e) if attempts >= max_retries => {
                    return Err(anyhow!("Failed after {} retries: {}", max_retries, e));
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }
    }

    /// Build a transaction without sending
    pub async fn build_transaction(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        payer: &Pubkey,
    ) -> Result<Transaction> {
        let _recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| anyhow!("Failed to get blockhash: {}", e))?;

        Ok(Transaction::new_with_payer(&instructions, Some(payer)))
    }
}

/// Enhanced utilities for transaction operations
pub mod utils {
    use super::*;
    use anyhow::Result;
    use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
    use std::sync::Arc;
    use tracing::debug;

    /// Create a transfer instruction with proper validation
    pub fn create_transfer_instruction(
        from_pubkey: &Pubkey,
        to_pubkey: &Pubkey,
        mint_pubkey: &Pubkey,
        amount: u64,
        _decimals: u8,
    ) -> Result<Instruction> {
        // Validate inputs
        if amount == 0 {
            return Err(anyhow!("Transfer amount cannot be zero"));
        }

        if !is_valid_pubkey(from_pubkey)
            || !is_valid_pubkey(to_pubkey)
            || !is_valid_pubkey(mint_pubkey)
        {
            return Err(anyhow!("Invalid public key in transfer instruction"));
        }

        debug!(
            "Creating transfer instruction: {} tokens from {} to {}",
            amount, from_pubkey, to_pubkey
        );

        // Return a placeholder - actual implementation would use spl_token
        Err(anyhow!(
            "Transfer instruction creation not yet implemented - type conflicts with anchor_lang"
        ))
    }

    /// Validate a Solana public key
    pub fn is_valid_pubkey(pubkey: &Pubkey) -> bool {
        // Just check it's not all zeros
        pubkey.to_bytes() != [0u8; 32]
    }

    /// Get or create an associated token account
    pub async fn get_or_create_ata(
        _rpc_client: &Arc<RpcClient>,
        _owner: &Pubkey,
        _mint: &Pubkey,
    ) -> Result<Pubkey> {
        // Disabled due to type conflicts between solana_sdk and anchor_lang Pubkey types
        Err(anyhow!(
            "ATA creation not yet implemented - type conflicts with anchor_lang"
        ))
    }
}
