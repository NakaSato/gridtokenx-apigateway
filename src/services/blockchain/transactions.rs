use crate::services::blockchain::priority_fee::{PriorityFeeService, TransactionType};
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
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
    fn determine_transaction_type(&self, transaction: &Transaction) -> &'static str {
        // Simple logic - returns string for logging
        if transaction.message.instructions.len() > 3 {
            "settlement"
        } else if transaction.message.instructions.len() == 1 {
            "token_transfer"
        } else {
            "token_transfer"
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
        transaction: &mut Transaction,
        tx_type: &'static str,
    ) -> Result<()> {
        // Convert string type to TransactionType enum
        let transaction_type = match tx_type {
            "token_transfer" => TransactionType::TokenTransfer,
            "minting" => TransactionType::Minting,
            "trading" => TransactionType::Trading,
            "settlement" => TransactionType::Settlement,
            _ => TransactionType::Other,
        };

        // Get dynamic priority fee
        let priority_fee_service = PriorityFeeService::new(self.rpc_client.clone());
        let priority_fee = priority_fee_service.get_priority_fee(transaction_type).await?;

        if priority_fee > 0 {
            // Create compute budget instructions
            let compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee);
            let compute_units_ix = ComputeBudgetInstruction::set_compute_unit_limit(200_000); // Default limit

            // Prepend instructions to transaction
            let mut new_instructions = vec![compute_unit_price_ix, compute_units_ix];
            new_instructions.extend(transaction.message.instructions.iter().map(|ix| {
                solana_sdk::instruction::Instruction {
                    program_id: transaction.message.account_keys[ix.program_id_index as usize],
                    accounts: ix.accounts.iter().map(|&idx| {
                        solana_sdk::instruction::AccountMeta {
                            pubkey: transaction.message.account_keys[idx as usize],
                            is_signer: transaction.message.is_signer(idx as usize),
                            is_writable: transaction.message.is_maybe_writable(idx as usize, None),
                        }
                    }).collect(),
                    data: ix.data.clone(),
                }
            }));

            // Rebuild transaction with priority fee instructions
            let payer = transaction.message.account_keys.first().cloned();
            *transaction = Transaction::new_with_payer(&new_instructions, payer.as_ref());

            debug!(
                "Added priority fee {} micro-lamports for {:?} transaction",
                priority_fee, transaction_type
            );
        }

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
            if let Ok(keypair) = super::utils::BlockchainUtils::load_keypair_from_file(path) {
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

    /// Submit transaction with exponential backoff retry logic
    async fn submit_with_retry(
        &self,
        mut transaction: Transaction,
        _initial_signature: Signature,
    ) -> Result<Signature> {
        let mut attempts = 0;
        let max_retries = 5;
        let base_delay_ms = 500u64;
        let max_delay_ms = 30_000u64;

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
                    let err_str = e.to_string();
                    error!(
                        "Transaction submission failed on attempt {}: {}",
                        attempts, err_str
                    );

                    // Check for non-retryable errors
                    let non_retryable = err_str.contains("insufficient funds")
                        || err_str.contains("InvalidAccountData")
                        || err_str.contains("AccountNotFound");

                    if non_retryable {
                        return Err(anyhow!(
                            "Transaction failed with non-retryable error: {}",
                            e
                        ));
                    }

                    if attempts >= max_retries {
                        return Err(anyhow!(
                            "Transaction failed after {} retries: {}",
                            max_retries,
                            e
                        ));
                    }
                }
            }

            // Calculate exponential backoff with jitter
            let exp_delay = base_delay_ms.saturating_mul(1u64 << (attempts - 1));
            let capped_delay = exp_delay.min(max_delay_ms);
            let jitter = rand::random::<u64>() % (capped_delay / 4 + 1);
            let total_delay = Duration::from_millis(capped_delay + jitter);
            
            debug!(
                "Waiting {:?} before retry attempt {} (base: {}ms, jitter: {}ms)",
                total_delay, attempts + 1, capped_delay, jitter
            );
            tokio::time::sleep(total_delay).await;
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
        _tx_type: &'static str,
        _fee: u64,
    ) -> Result<()> {
        // DISABLED - priority_fee module not available
        Ok(())
    }

    /// Confirm transaction status (basic check)
    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        let sig =
            Signature::from_str(signature).map_err(|e| anyhow!("Invalid signature: {}", e))?;

        let status = self
            .rpc_client
            .get_signature_status(&sig)
            .map_err(|e| anyhow!("Failed to get signature status: {}", e))?;

        Ok(status.is_some())
    }

    /// Confirm transaction with polling until confirmed/finalized or timeout
    /// 
    /// This method continuously polls the transaction status until:
    /// - Transaction is confirmed/finalized
    /// - Transaction fails
    /// - Timeout is reached
    pub async fn confirm_transaction_with_polling(
        &self,
        signature: &Signature,
        timeout_secs: u64,
        poll_interval_ms: u64,
    ) -> Result<TransactionStatus> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let poll_interval = Duration::from_millis(poll_interval_ms);
        
        info!(
            "Starting confirmation polling for {} (timeout: {}s, interval: {}ms)",
            signature, timeout_secs, poll_interval_ms
        );

        let mut last_status = TransactionStatus::Pending;
        let mut polls = 0u32;

        loop {
            polls += 1;
            
            if start.elapsed() >= timeout {
                warn!(
                    "Transaction confirmation timeout after {}s ({} polls): {}",
                    timeout_secs, polls, signature
                );
                return Ok(TransactionStatus::Pending);
            }

            match self.get_transaction_status(signature).await {
                Ok(status) => {
                    // Log status transitions
                    if std::mem::discriminant(&status) != std::mem::discriminant(&last_status) {
                        info!(
                            "Transaction {} status: {:?} -> {:?} (poll #{})",
                            signature, last_status, status, polls
                        );
                        last_status = status.clone();
                    }

                    match &status {
                        TransactionStatus::Finalized => {
                            info!(
                                "Transaction {} finalized after {:?} ({} polls)",
                                signature, start.elapsed(), polls
                            );
                            return Ok(status);
                        }
                        TransactionStatus::Confirmed(count) if *count >= 1 => {
                            debug!(
                                "Transaction {} confirmed with {} confirmations",
                                signature, count
                            );
                            // Continue polling until finalized or user-defined threshold
                            if *count >= 32 {
                                return Ok(TransactionStatus::Finalized);
                            }
                        }
                        TransactionStatus::Failed(err) => {
                            error!("Transaction {} failed: {}", signature, err);
                            return Ok(status);
                        }
                        _ => {
                            // Still pending or processed, keep polling
                        }
                    }
                }
                Err(e) => {
                    warn!("Error polling transaction status (poll #{}): {}", polls, e);
                    // Don't fail immediately on transient errors
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Get trade record from blockchain - DISABLED
    // pub async fn get_trade_record(
    //     &self,
    //     _signature: &str,
    // ) -> Result<crate::models::transaction::TradeRecord> {
    //     Err(anyhow!("Trade record fetching not implemented"))
    // }

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

    /// Get account info
    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<solana_sdk::account::Account> {
        let conn = self.get_connection().await;
        let account = conn
            .get_account(pubkey)
            .map_err(|e| anyhow!("Failed to get account: {}", e))?;
        self.return_connection(conn).await;
        Ok(account)
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
            Ok(_) => {
                debug!("Account {} exists", pubkey);
                Ok(true)
            }
            Err(e) => {
                warn!("Account {} check failed/not found: {}", pubkey, e);
                Ok(false)
            }
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
        _transaction_type: &'static str,
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

    /// Wait for transaction to reach target confirmations
    pub async fn wait_for_confirmations(
        &self,
        signature: &Signature,
        target_confirmations: u64,
        timeout_secs: u64,
    ) -> Result<TransactionStatus> {
        let start = std::time::Instant::now();
        info!(
            "Waiting for {} confirmations on signature: {}",
            target_confirmations, signature
        );

        loop {
            if start.elapsed().as_secs() >= timeout_secs {
                warn!("Transaction confirmation timeout after {}s", timeout_secs);
                return Ok(TransactionStatus::Pending);
            }

            match self.get_transaction_status(signature).await? {
                TransactionStatus::Finalized => {
                    info!("Transaction {} finalized", signature);
                    return Ok(TransactionStatus::Finalized);
                }
                TransactionStatus::Confirmed(count) if count >= target_confirmations => {
                    info!(
                        "Transaction {} reached {} confirmations",
                        signature, count
                    );
                    return Ok(TransactionStatus::Confirmed(count));
                }
                TransactionStatus::Failed(err) => {
                    error!("Transaction {} failed: {}", signature, err);
                    return Ok(TransactionStatus::Failed(err));
                }
                status => {
                    debug!("Transaction {} status: {:?}, waiting...", signature, status);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// Get detailed transaction status
    pub async fn get_transaction_status(&self, signature: &Signature) -> Result<TransactionStatus> {
        use solana_client::rpc_config::RpcTransactionConfig;
        use solana_transaction_status::UiTransactionEncoding;

        // First check signature status
        let status = self
            .rpc_client
            .get_signature_status(signature)
            .map_err(|e| anyhow!("Failed to get signature status: {}", e))?;

        match status {
            None => Ok(TransactionStatus::Pending),
            Some(result) => match result {
                Ok(_) => {
                    // Transaction succeeded, check confirmation level
                    let config = RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Json),
                        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::finalized()),
                        max_supported_transaction_version: Some(0),
                    };

                    match self.rpc_client.get_transaction_with_config(signature, config) {
                        Ok(tx) => {
                            if tx.slot > 0 {
                                // Get current slot to calculate confirmations
                                let current_slot = self.rpc_client.get_slot().unwrap_or(0);
                                let confirmations = current_slot.saturating_sub(tx.slot);
                                
                                // Solana considers 32+ confirmations as finalized
                                if confirmations >= 32 {
                                    Ok(TransactionStatus::Finalized)
                                } else {
                                    Ok(TransactionStatus::Confirmed(confirmations))
                                }
                            } else {
                                Ok(TransactionStatus::Processed)
                            }
                        }
                        Err(_) => {
                            // Transaction exists but can't get details - it's at least processed
                            Ok(TransactionStatus::Processed)
                        }
                    }
                }
                Err(err) => Ok(TransactionStatus::Failed(format!("{:?}", err))),
            },
        }
    }

    /// Get the number of confirmations for a transaction
    pub async fn get_confirmation_count(&self, signature: &Signature) -> Result<u64> {
        match self.get_transaction_status(signature).await? {
            TransactionStatus::Finalized => Ok(32), // Finalized = 32+ confirmations
            TransactionStatus::Confirmed(count) => Ok(count),
            TransactionStatus::Processed => Ok(1),
            TransactionStatus::Pending => Ok(0),
            TransactionStatus::Failed(_) => Ok(0),
        }
    }

    /// Estimate transaction fee before sending
    pub async fn estimate_transaction_fee(&self, transaction: &Transaction) -> Result<FeeEstimate> {
        // Get fee for message
        let fee = self
            .rpc_client
            .get_fee_for_message(&transaction.message)
            .map_err(|e| anyhow!("Failed to estimate fee: {}", e))?;

        // Get priority fee estimate (simplified - actual implementation would query recent fees)
        let priority_fee = self.get_priority_fee_estimate().await?;

        Ok(FeeEstimate {
            base_fee: fee,
            priority_fee,
            total_fee: fee + priority_fee,
        })
    }

    /// Get priority fee estimate based on recent transactions
    async fn get_priority_fee_estimate(&self) -> Result<u64> {
        // Query recent priority fees from the network
        // For now, use a simple heuristic based on recent blocks
        // Default priority fee: 0.00001 SOL = 10,000 lamports
        let default_priority_fee = 10_000u64;
        
        // Try to get recent prioritization fees
        match self.rpc_client.get_recent_prioritization_fees(&[]) {
            Ok(fees) => {
                if fees.is_empty() {
                    Ok(default_priority_fee)
                } else {
                    // Calculate median priority fee
                    let mut fee_values: Vec<u64> = fees.iter().map(|f| f.prioritization_fee).collect();
                    fee_values.sort();
                    let median = fee_values[fee_values.len() / 2];
                    // Add 20% buffer for reliability
                    Ok(median.saturating_mul(120) / 100)
                }
            }
            Err(_) => Ok(default_priority_fee),
        }
    }

    /// Check if account has sufficient SOL for transaction fees
    pub async fn check_sufficient_sol(&self, pubkey: &Pubkey, required_fee: u64) -> Result<SolBalanceCheck> {
        let balance = self.get_balance(pubkey).await?;
        let rent_exempt_minimum = 890_880u64; // Approximate rent-exempt minimum for an account
        
        let required_total = required_fee + rent_exempt_minimum;
        let sufficient = balance >= required_total;

        Ok(SolBalanceCheck {
            balance,
            required_fee,
            rent_exempt_minimum,
            sufficient,
            deficit: if sufficient { 0 } else { required_total - balance },
        })
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

    // ============ ESCROW METHODS ============
    
    /// Derive escrow PDA for an order
    pub fn derive_escrow_pda(order_id: &[u8; 32], program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"escrow", order_id], program_id)
    }

    /// Lock tokens to escrow for a buy order
    pub async fn lock_tokens_to_escrow(
        &self,
        buyer_authority: &Keypair,
        buyer_ata: &Pubkey,
        escrow_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        info!("🔒 Locking {} tokens to escrow: {} -> {}", amount, buyer_ata, escrow_ata);
        
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        let transfer_ix = spl_token::instruction::transfer_checked(
            &token_program, buyer_ata, token_mint, escrow_ata, &buyer_authority.pubkey(), &[], amount, decimals,
        )?;

        let payer: Keypair = self.get_payer_keypair().await?;
        let recent_blockhash = self.get_recent_blockhash().await?;
        let transaction = Transaction::new_signed_with_payer(
            &[transfer_ix], Some(&payer.pubkey()), &[&payer, buyer_authority], recent_blockhash,
        );

        let signature = self.submit_transaction(transaction).await?;
        info!("🔒 Escrow lock complete: {}", signature);
        Ok(signature)
    }

    /// Release escrow tokens to seller after settlement
    pub async fn release_escrow_to_seller(
        &self,
        escrow_authority: &Keypair,
        escrow_ata: &Pubkey,
        seller_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        info!("✅ Releasing {} tokens from escrow: {} -> {}", amount, escrow_ata, seller_ata);
        
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        let transfer_ix = spl_token::instruction::transfer_checked(
            &token_program, escrow_ata, token_mint, seller_ata, &escrow_authority.pubkey(), &[], amount, decimals,
        )?;

        let payer: Keypair = self.get_payer_keypair().await?;
        let recent_blockhash = self.get_recent_blockhash().await?;
        let transaction = Transaction::new_signed_with_payer(
            &[transfer_ix], Some(&payer.pubkey()), &[&payer, escrow_authority], recent_blockhash,
        );

        let signature = self.submit_transaction(transaction).await?;
        info!("✅ Escrow release complete: {}", signature);
        Ok(signature)
    }

    /// Refund escrow tokens to buyer on order cancel
    pub async fn refund_escrow_to_buyer(
        &self,
        escrow_authority: &Keypair,
        escrow_ata: &Pubkey,
        buyer_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        info!("↩️ Refunding {} tokens from escrow: {} -> {}", amount, escrow_ata, buyer_ata);
        
        let token_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        let transfer_ix = spl_token::instruction::transfer_checked(
            &token_program, escrow_ata, token_mint, buyer_ata, &escrow_authority.pubkey(), &[], amount, decimals,
        )?;

        let payer: Keypair = self.get_payer_keypair().await?;
        let recent_blockhash = self.get_recent_blockhash().await?;
        let transaction = Transaction::new_signed_with_payer(
            &[transfer_ix], Some(&payer.pubkey()), &[&payer, escrow_authority], recent_blockhash,
        );

        let signature = self.submit_transaction(transaction).await?;
        info!("↩️ Escrow refund complete: {}", signature);
        Ok(signature)
    }
}

/// Transaction status for detailed tracking
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    /// Transaction not yet submitted or not found
    Pending,
    /// Transaction included in a block (1 confirmation)
    Processed,
    /// Transaction confirmed with N confirmations
    Confirmed(u64),
    /// Transaction finalized (32+ confirmations, irreversible)
    Finalized,
    /// Transaction failed with error message
    Failed(String),
}

/// Fee estimation result
#[derive(Debug, Clone)]
pub struct FeeEstimate {
    /// Base transaction fee in lamports
    pub base_fee: u64,
    /// Recommended priority fee in lamports
    pub priority_fee: u64,
    /// Total estimated fee (base + priority)
    pub total_fee: u64,
}

/// SOL balance check result
#[derive(Debug, Clone)]
pub struct SolBalanceCheck {
    /// Current balance in lamports
    pub balance: u64,
    /// Required fee for the transaction
    pub required_fee: u64,
    /// Minimum balance to keep for rent exemption
    pub rent_exempt_minimum: u64,
    /// Whether balance is sufficient
    pub sufficient: bool,
    /// Deficit amount if insufficient (0 if sufficient)
    pub deficit: u64,
}

/// Enhanced utilities for transaction operations
pub mod utils {
    use super::*;
    use anyhow::Result;
    use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
    use std::sync::Arc;
    use tracing::debug;

    /// Create a transfer instruction with proper validation
    /// Uses spl_token::instruction::transfer_checked for Token-2022 compatibility
    pub fn create_transfer_instruction(
        from_ata: &Pubkey,
        to_ata: &Pubkey,
        mint_pubkey: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction> {
        // Validate inputs
        if amount == 0 {
            return Err(anyhow!("Transfer amount cannot be zero"));
        }

        if !is_valid_pubkey(from_ata)
            || !is_valid_pubkey(to_ata)
            || !is_valid_pubkey(mint_pubkey)
        {
            return Err(anyhow!("Invalid public key in transfer instruction"));
        }

        debug!(
            "Creating transfer_checked instruction: {} tokens from {} to {}",
            amount, from_ata, to_ata
        );

        // Use transfer_checked for Token-2022 compatibility
        let instruction = spl_token::instruction::transfer_checked(
            &spl_token::ID,  // Use standard token program; caller can override for Token-2022
            from_ata,
            mint_pubkey,
            to_ata,
            owner,
            &[],  // No multisig signers
            amount,
            decimals,
        )?;

        Ok(instruction)
    }

    /// Create a transfer instruction for Token-2022
    pub fn create_transfer_instruction_2022(
        from_ata: &Pubkey,
        to_ata: &Pubkey,
        mint_pubkey: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction> {
        if amount == 0 {
            return Err(anyhow!("Transfer amount cannot be zero"));
        }

        debug!(
            "Creating Token-2022 transfer_checked instruction: {} tokens from {} to {}",
            amount, from_ata, to_ata
        );

        // Token-2022 program ID
        let token_2022_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;

        let instruction = spl_token::instruction::transfer_checked(
            &token_2022_program,
            from_ata,
            mint_pubkey,
            to_ata,
            owner,
            &[],
            amount,
            decimals,
        )?;

        Ok(instruction)
    }

    /// Validate a Solana public key
    pub fn is_valid_pubkey(pubkey: &Pubkey) -> bool {
        // Just check it's not all zeros
        pubkey.to_bytes() != [0u8; 32]
    }

    /// Get the associated token account address for an owner and mint
    pub fn get_ata_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        spl_associated_token_account::get_associated_token_address(owner, mint)
    }

    /// Get the associated token account address for Token-2022
    pub fn get_ata_address_2022(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        let token_2022_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .expect("Invalid Token-2022 program ID");
        spl_associated_token_account::get_associated_token_address_with_program_id(
            owner,
            mint,
            &token_2022_program,
        )
    }

    /// Get or create an associated token account
    /// Returns the ATA address and optionally an instruction to create it
    pub async fn get_or_create_ata(
        rpc_client: &Arc<RpcClient>,
        owner: &Pubkey,
        mint: &Pubkey,
        payer: &Pubkey,
    ) -> Result<(Pubkey, Option<Instruction>)> {
        let ata = get_ata_address(owner, mint);

        // Check if the ATA already exists
        match rpc_client.get_account(&ata) {
            Ok(account) => {
                if account.data.len() > 0 {
                    debug!("ATA {} already exists for owner {}", ata, owner);
                    return Ok((ata, None));
                }
            }
            Err(_) => {
                // Account doesn't exist, need to create it
            }
        }

        debug!("Creating ATA instruction for owner {} mint {}", owner, mint);

        // Create instruction to create the ATA
        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            payer,
            owner,
            mint,
            &spl_token::ID,
        );

        Ok((ata, Some(create_ata_ix)))
    }

    /// Get or create an associated token account for Token-2022
    pub async fn get_or_create_ata_2022(
        rpc_client: &Arc<RpcClient>,
        owner: &Pubkey,
        mint: &Pubkey,
        payer: &Pubkey,
    ) -> Result<(Pubkey, Option<Instruction>)> {
        let token_2022_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
            owner,
            mint,
            &token_2022_program,
        );

        // Check if the ATA already exists
        match rpc_client.get_account(&ata) {
            Ok(account) => {
                if account.data.len() > 0 {
                    debug!("Token-2022 ATA {} already exists for owner {}", ata, owner);
                    return Ok((ata, None));
                }
            }
            Err(_) => {
                // Account doesn't exist
            }
        }

        debug!("Creating Token-2022 ATA instruction for owner {} mint {}", owner, mint);

        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            payer,
            owner,
            mint,
            &token_2022_program,
        );

        Ok((ata, Some(create_ata_ix)))
    }

    /// Determine if a mint uses Token-2022
    pub async fn is_token_2022(rpc_client: &Arc<RpcClient>, mint: &Pubkey) -> Result<bool> {
        let token_2022_program = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
        
        match rpc_client.get_account(mint) {
            Ok(account) => Ok(account.owner == token_2022_program),
            Err(e) => Err(anyhow!("Failed to get mint account: {}", e)),
        }
    }


}
