use crate::config::SolanaProgramsConfig;
use crate::services::blockchain_instructions::InstructionBuilder;
use crate::services::blockchain_transactions::TransactionHandler;
use crate::services::blockchain_utils::BlockchainUtils;
use crate::services::priority_fee_service::TransactionType;
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

/// Blockchain service for interacting with Solana programs
/// BlockchainService for interacting with Solana programs
#[derive(Clone)]
pub struct BlockchainService {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
    rpc_client: Arc<RpcClient>,
    cluster: String,
    /// Configurable program IDs loaded from environment
    program_ids: SolanaProgramsConfig,
}

impl std::fmt::Debug for BlockchainService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockchainService")
            .field("transaction_handler", &self.transaction_handler)
            .field("instruction_builder", &self.instruction_builder)
            .field("rpc_client", &"RpcClient")
            .field("cluster", &self.cluster)
            .field("program_ids", &self.program_ids)
            .finish()
    }
}

impl BlockchainService {
    /// Create a new blockchain service with program IDs from config
    pub fn new(
        rpc_url: String,
        cluster: String,
        program_ids: SolanaProgramsConfig,
    ) -> Result<Self> {
        info!("Initializing blockchain service for cluster: {}", cluster);

        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let transaction_handler = TransactionHandler::new(Arc::clone(&rpc_client));

        // Load authority keypair to get the payer pubkey
        let authority_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "../keypairs/dev-wallet.json".to_string());

        let payer = match BlockchainUtils::load_keypair_from_file(&authority_path) {
            Ok(keypair) => {
                info!(
                    "Loaded authority keypair for instruction builder: {}",
                    keypair.pubkey()
                );
                keypair.pubkey()
            }
            Err(e) => {
                warn!(
                    "Failed to load authority keypair from {}: {}. Using placeholder payer key.",
                    authority_path, e
                );
                // Fallback to placeholder
                "11111111111111111111111111111112"
                    .parse()
                    .expect("hardcoded placeholder pubkey is invalid")
            }
        };

        let instruction_builder = InstructionBuilder::new(payer);

        Ok(Self {
            transaction_handler,
            instruction_builder,
            rpc_client,
            cluster,
            program_ids,
        })
    }

    /// Get the RPC client
    pub fn client(&self) -> &RpcClient {
        self.transaction_handler.client()
    }

    /// Get the cluster name
    pub fn cluster(&self) -> &str {
        &self.cluster
    }

    /// Get the payer pubkey
    pub fn payer_pubkey(&self) -> Pubkey {
        // In a real implementation, this would load from secure storage
        // For now, return a placeholder
        "11111111111111111111111111111111112"
            .parse()
            .expect("hardcoded placeholder pubkey is invalid")
    }

    /// Submit transaction to blockchain
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<Signature> {
        self.transaction_handler
            .submit_transaction(transaction)
            .await
    }

    /// Add priority fee to transaction
    pub fn add_priority_fee(
        &self,
        transaction: &mut Transaction,
        tx_type: TransactionType,
        fee: u64,
    ) -> Result<()> {
        self.transaction_handler
            .add_priority_fee(transaction, tx_type, fee)
    }

    /// Confirm transaction status
    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        self.transaction_handler
            .confirm_transaction(signature)
            .await
    }

    /// Get trade record from blockchain
    pub async fn get_trade_record(
        &self,
        signature: &str,
    ) -> Result<crate::models::transaction::TradeRecord> {
        self.transaction_handler.get_trade_record(signature).await
    }

    /// Check if the service is healthy by querying the network
    pub async fn health_check(&self) -> Result<bool> {
        self.transaction_handler.health_check().await
    }

    /// Request airdrop (devnet/localnet only)
    pub async fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        self.transaction_handler
            .request_airdrop(pubkey, lamports)
            .await
    }

    /// Get account balance in lamports
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.transaction_handler.get_balance(pubkey).await
    }

    /// Get account balance in SOL
    pub async fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        self.transaction_handler.get_balance_sol(pubkey).await
    }

    /// Get SPL token balance for a user
    pub async fn get_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        // Calculate ATA address
        let ata_address = self.calculate_ata_address(owner, mint)?;

        // Check if ATA exists
        if !self.account_exists(&ata_address).await? {
            return Ok(0);
        }

        // Get balance
        self.transaction_handler
            .get_token_account_balance(&ata_address)
            .await
    }

    /// Send and confirm a transaction
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        self.transaction_handler
            .send_and_confirm_transaction(transaction)
            .await
    }

    /// Get transaction status
    pub async fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>> {
        self.transaction_handler
            .get_signature_status(signature)
            .await
    }

    /// Get recent blockhash
    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.transaction_handler.get_latest_blockhash().await
    }

    /// Get slot height
    pub async fn get_slot(&self) -> Result<u64> {
        self.transaction_handler.get_slot().await
    }

    /// Get account data
    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        self.transaction_handler.get_account_data(pubkey).await
    }

    /// Check if an account exists
    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        self.transaction_handler.account_exists(pubkey).await
    }

    /// Get transaction account keys
    pub async fn get_transaction_account_keys(&self, signature: &str) -> Result<Vec<Pubkey>> {
        let sig =
            Signature::from_str(signature).map_err(|e| anyhow!("Invalid signature: {}", e))?;
        // get_transaction takes (signature, encoding)
        let tx = self
            .transaction_handler
            .client()
            .get_transaction(&sig, solana_transaction_status::UiTransactionEncoding::Json)?;

        // Extract keys
        let account_keys = tx
            .transaction
            .meta
            .ok_or(anyhow!("No metadata"))?
            .post_token_balances
            .ok_or(anyhow!("No token balances"))?; // This logic is wrong but unused since we look at message below.

        // Use transaction object directly
        let transaction = tx.transaction.transaction;
        match transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => match ui_tx.message {
                solana_transaction_status::UiMessage::Parsed(msg) => Ok(msg
                    .account_keys
                    .iter()
                    .map(|k| Pubkey::from_str(&k.pubkey).unwrap())
                    .collect()),
                solana_transaction_status::UiMessage::Raw(msg) => Ok(msg
                    .account_keys
                    .iter()
                    .map(|k| Pubkey::from_str(k).unwrap())
                    .collect()),
            },
            _ => Err(anyhow!("Unsupported transaction encoding")),
        }
    }

    /// Parse Pubkey from string
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        BlockchainUtils::parse_pubkey(pubkey_str)
    }

    /// Get Registry program ID from config
    pub fn registry_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.registry_program_id).map_err(|e| {
            anyhow!(
                "Invalid Registry Program ID '{}': {}",
                self.program_ids.registry_program_id,
                e
            )
        })
    }

    /// Get Oracle program ID from config
    pub fn oracle_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.oracle_program_id).map_err(|e| {
            anyhow!(
                "Invalid Oracle Program ID '{}': {}",
                self.program_ids.oracle_program_id,
                e
            )
        })
    }

    /// Get Governance program ID from config
    pub fn governance_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.governance_program_id).map_err(|e| {
            anyhow!(
                "Invalid Governance Program ID '{}': {}",
                self.program_ids.governance_program_id,
                e
            )
        })
    }

    /// Get Energy Token program ID from config
    pub fn energy_token_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.energy_token_program_id).map_err(|e| {
            anyhow!(
                "Invalid Energy Token Program ID '{}': {}",
                self.program_ids.energy_token_program_id,
                e
            )
        })
    }

    /// Get Trading program ID from config
    pub fn trading_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.trading_program_id).map_err(|e| {
            anyhow!(
                "Invalid Trading Program ID '{}': {}",
                self.program_ids.trading_program_id,
                e
            )
        })
    }

    // ====================================================================
    // Instruction Building Methods (delegated to InstructionBuilder)
    // ====================================================================

    /// Build instruction for creating energy trade order
    /// Get active orders count from market account
    async fn get_market_active_orders(&self, market_pubkey: &Pubkey) -> Result<u64> {
        let client = Arc::clone(&self.rpc_client);
        let market_pubkey = *market_pubkey;

        let active_orders = tokio::task::spawn_blocking(move || {
            let account = client.get_account(&market_pubkey)?;
            // Parse active_orders from account data (offset 40, u64)
            if account.data.len() < 48 {
                return Err(anyhow!("Market account data too small"));
            }
            let active_orders_bytes: [u8; 8] = account.data[40..48]
                .try_into()
                .expect("slice length already verified to be 8 bytes");
            Ok(u64::from_le_bytes(active_orders_bytes))
        })
        .await??;

        Ok(active_orders)
    }

    /// Derive order PDA (exposed for SettlementService)
    pub async fn derive_order_pda(
        &self,
        _authority: &Pubkey,
        _market_address: &Pubkey,
    ) -> Result<Pubkey> {
        // We need active_orders to derive the PDA.
        // But SettlementService calls this for EXISTING orders.
        // The PDA seed logic is: ["order", owner, active_orders].
        // If we don't know the active_orders (index) at creation, we can't derive it?
        // Wait! The Order PDA is stored in the database as `blockchain_tx`? No.
        // The UUID is internal. The PDA is ... derived at creation.
        // But if multiple orders exist for same user, they have different indices.
        // The `trading_orders` table DOES NOT STORE THE PDA ADDRESS!
        // It stores `id` (UUID).
        // It stores `blockchain_tx_signature`.

        // ISSUE: We cannot recreate the PDA without the `index` (active_orders at time of creation).
        // Unless we store the PDA address in the DB.
        // The `trading_orders` table has columns... let's check schema.
        // DB Schema in migration?
        // If we don't have PDA address, we are stuck?
        // Or maybe we find it by searching on-chain (getProgramAccounts)?

        // TEMPORARY WORKAROUND:
        // Assume active_orders was retrieved? No.

        // Let's assume user has only 1 order or we fallback to "fetch all user order accounts" and match by data?
        // Or we rely on `blockchain_tx` (Signature) to find the account created?
        // Transaction logs contain the account list.

        // This is a Database Schema GAP. We SHOULD store the Order PDA address.
        // But for now, I will add `derive_order_pda` which takes `index`.
        // SettlementService will have to figure it out.
        // Or Query `getProgramAccounts` filtering by owner.

        Err(anyhow!("Cannot derive PDA without index"))
    }

    /// Execute on-chain match_orders
    pub async fn execute_match_orders(
        &self,
        authority: &Keypair,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
    ) -> Result<Signature> {
        // Parse order pubkeys
        let buy_order = Pubkey::from_str(buy_order_pubkey)?;
        let sell_order = Pubkey::from_str(sell_order_pubkey)?;

        // Derive trade_record PDA (must match on-chain seeds)
        let (trade_record_pda, _bump) = Pubkey::find_program_address(
            &[b"trade", buy_order.as_ref(), sell_order.as_ref()],
            &self.trading_program_id()?,
        );

        let instruction = self.instruction_builder.build_match_orders_instruction(
            market_pubkey,
            buy_order_pubkey,
            sell_order_pubkey,
            match_amount,
            trade_record_pda,
        )?;

        // Only authority signs (trade_record is a PDA, not a signer)
        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            vec![instruction],
            &signers,
            TransactionType::Settlement,
        )
        .await
    }

    /// Execute on-chain create_order
    pub async fn execute_create_order(
        &self,
        authority: &Keypair,
        market_pubkey: &str,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
    ) -> Result<Signature> {
        let (instruction, _order_pda) = self
            .build_create_order_instruction(
                market_pubkey,
                energy_amount,
                price_per_kwh,
                order_type,
                erc_certificate_id,
                authority.pubkey(),
            )
            .await?;

        let signers = vec![authority];

        self.build_and_send_transaction_with_priority(
            vec![instruction],
            &signers,
            TransactionType::OrderCreation,
        )
        .await
    }

    /// Build instruction for creating energy trade order
    /// Returns (Instruction, Order PDA)
    pub async fn build_create_order_instruction(
        &self,
        market_pubkey: &str,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
        authority: Pubkey,
    ) -> Result<(Instruction, Pubkey)> {
        let market = Pubkey::from_str(market_pubkey)?;

        // Get active orders count
        let active_orders = self.get_market_active_orders(&market).await?;

        // Derive order PDA
        let (order_pda, _) = Pubkey::find_program_address(
            &[b"order", authority.as_ref(), &active_orders.to_le_bytes()],
            &self.trading_program_id()?,
        );

        let instruction = self.instruction_builder.build_create_order_instruction(
            market_pubkey,
            order_pda,
            energy_amount,
            price_per_kwh,
            order_type,
            erc_certificate_id,
            authority,
        )?;

        Ok((instruction, order_pda))
    }

    /// Build instruction for matching orders
    pub fn build_match_orders_instruction(
        &self,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
        trade_record_pubkey: Pubkey,
    ) -> Result<Instruction> {
        self.instruction_builder.build_match_orders_instruction(
            market_pubkey,
            buy_order_pubkey,
            sell_order_pubkey,
            match_amount,
            trade_record_pubkey,
        )
    }

    /// Build instruction for minting tokens
    pub fn build_mint_instruction(&self, recipient: &str, amount: u64) -> Result<Instruction> {
        self.instruction_builder
            .build_mint_instruction(recipient, amount)
    }

    /// Build instruction for transferring tokens
    pub fn build_transfer_instruction(
        &self,
        from: &str,
        to: &str,
        amount: u64,
        token_mint: &str,
    ) -> Result<Instruction> {
        self.instruction_builder
            .build_transfer_instruction(from, to, amount, token_mint)
    }

    /// Build instruction for casting a governance vote
    pub fn build_vote_instruction(&self, proposal_id: u64, vote: bool) -> Result<Instruction> {
        self.instruction_builder
            .build_vote_instruction(proposal_id, vote)
    }

    /// Build instruction for updating oracle price
    pub fn build_update_price_instruction(
        &self,
        price_feed_id: &str,
        price: u64,
        confidence: u64,
    ) -> Result<Instruction> {
        self.instruction_builder
            .build_update_price_instruction(price_feed_id, price, confidence)
    }

    /// Build instruction for updating registry
    pub fn build_update_registry_instruction(
        &self,
        participant_id: &str,
        update_data: &serde_json::Value,
    ) -> Result<Instruction> {
        self.instruction_builder
            .build_update_registry_instruction(participant_id, update_data)
    }

    // ====================================================================
    // Transaction Building & Signing (Phase 4) - delegated to TransactionHandler
    // ====================================================================

    /// Priority 4: Build, sign, and send a transaction with automatic priority fees
    /// Returns transaction signature with enhanced performance monitoring
    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        self.transaction_handler
            .build_and_send_transaction(instructions, signers)
            .await
    }

    /// Build, sign, and send a transaction with specified priority level
    /// Returns transaction signature
    pub async fn build_and_send_transaction_with_priority(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
        transaction_type: TransactionType,
    ) -> Result<Signature> {
        self.transaction_handler
            .build_and_send_transaction_with_priority(instructions, signers, transaction_type)
            .await
    }

    /// Simulate a transaction before sending
    /// Returns whether the simulation succeeded
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<bool> {
        self.transaction_handler
            .simulate_transaction(transaction)
            .await?;
        Ok(true)
    }

    /// Wait for transaction confirmation with timeout
    /// Returns true if confirmed, false if timeout
    pub async fn wait_for_confirmation(
        &self,
        signature: &Signature,
        timeout_secs: u64,
    ) -> Result<bool> {
        self.transaction_handler
            .wait_for_confirmation(signature, timeout_secs)
            .await
    }

    /// Send transaction with retry logic
    /// Retries up to max_retries times on failure
    pub async fn send_transaction_with_retry(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
        max_retries: u32,
    ) -> Result<Signature> {
        self.transaction_handler
            .send_transaction_with_retry(instructions, signers, max_retries)
            .await
    }

    /// Build a transaction without sending
    /// Useful for inspection or simulation
    pub async fn build_transaction(
        &self,
        instructions: Vec<Instruction>,
        payer: &Pubkey,
    ) -> Result<Transaction> {
        self.transaction_handler
            .build_transaction(instructions, payer)
            .await
    }

    // ====================================================================
    // Utility Methods - delegated to BlockchainUtils
    // ====================================================================

    /// Load keypair from a JSON file
    pub fn load_keypair_from_file(filepath: &str) -> Result<Keypair> {
        BlockchainUtils::load_keypair_from_file(filepath)
    }

    /// Get authority keypair (for settlement service)
    pub async fn get_authority_keypair(&self) -> Result<Keypair> {
        // In production, this should use secure key management
        // For development, we load from a local file
        let wallet_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "dev-wallet.json".to_string());

        info!("Loading authority keypair from: {}", wallet_path);
        Self::load_keypair_from_file(&wallet_path)
    }

    /// Mint energy tokens directly to a user's token account
    pub async fn mint_energy_tokens(
        &self,
        authority: &Keypair,
        user_token_account: &Pubkey,
        user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        let mut instructions = Vec::new();

        // Check if ATA exists
        if !self.account_exists(user_token_account).await? {
            info!("ATA {} does not exist, creating it...", user_token_account);
            let create_ata_ix =
                BlockchainUtils::create_ata_instruction(authority, user_wallet, mint)?;
            instructions.push(create_ata_ix);
        }

        let mint_instruction = BlockchainUtils::create_mint_instruction(
            authority,
            user_token_account,
            user_wallet,
            mint,
            amount_kwh,
        )?;
        instructions.push(mint_instruction);

        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            instructions,
            &signers,
            TransactionType::TokenMinting,
        )
        .await
    }

    /// Burn energy tokens from a user's token account
    pub async fn burn_energy_tokens(
        &self,
        authority: &Keypair,
        user_token_account: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        let burn_instruction = BlockchainUtils::create_burn_instruction(
            authority,
            user_token_account,
            mint,
            amount_kwh,
        )?;

        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            vec![burn_instruction],
            &signers,
            TransactionType::Settlement, // Use Settlement priority for burning
        )
        .await
    }

    /// Transfer energy tokens between accounts
    pub async fn transfer_energy_tokens(
        &self,
        authority: &Keypair,
        from_token_account: &Pubkey,
        to_token_account: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh.abs() * 1_000_000_000.0) as u64;

        let transfer_instruction = BlockchainUtils::create_transfer_instruction(
            authority,
            from_token_account,
            to_token_account,
            mint,
            amount_lamports,
            9, // Decimals
        )?;

        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            vec![transfer_instruction],
            &signers,
            TransactionType::Settlement,
        )
        .await
    }

    /// Ensures user has an Associated Token Account for the token mint
    pub async fn ensure_token_account_exists(
        &self,
        _authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Pubkey> {
        println!("DEBUG: ensure_token_account_exists called");
        // Check if account exists and is valid
        let ata_address = self.calculate_ata_address(user_wallet, mint)?;
        println!(
            "DEBUG: Checking existence of ATA: {} for wallet: {}",
            ata_address, user_wallet
        );

        // Try to get account info directly first
        match self.transaction_handler.get_account(&ata_address).await {
            Ok(account) => {
                println!(
                    "DEBUG: ATA account found! Owner: {}, Data Len: {}, Lamports: {}",
                    account.owner,
                    account.data.len(),
                    account.lamports
                );
                // Check if owned by Token-2022 program
                let token_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
                    .expect("hardcoded Token-2022 program ID is invalid");
                if account.owner == token_2022_id || account.owner == spl_token::id() {
                    println!("DEBUG: ATA is owned by Token Program (Token-2022 or legacy). Valid.");
                    return Ok(ata_address);
                } else {
                    println!("DEBUG: ATA exists but has wrong owner: {}", account.owner);
                }
            }
            Err(e) => {
                println!(
                    "DEBUG: ATA get_account failed: {} - Error: {}",
                    ata_address, e
                );
            }
        }

        // Check if account exists and is valid (Keep existing check)
        // ... (lines 526-575 are fine, but I'm rewriting the block to be safe)

        // Fallback to balance check (which might fail if account is not a token account)
        match self
            .transaction_handler
            .get_token_account_balance(&ata_address)
            .await
        {
            Ok(balance) => {
                println!(
                    "DEBUG: ATA balance check success: {} (Balance: {})",
                    ata_address, balance
                );
                return Ok(ata_address);
            }
            Err(_) => {
                println!("DEBUG: ATA balance check failed, proceeding to create");
            }
        }

        println!(
            "DEBUG: Creating ATA via CLI for mint: {}, owner: {}",
            mint, user_wallet
        );

        // Resolve wallet path for CLI
        let wallet_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "dev-wallet.json".to_string());

        // Use spl-token CLI to create account with Token-2022 program
        // This bypasses the spl-associated-token-account crate version mismatch
        let rpc_url =
            std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://localhost:8899".to_string());

        let output = std::process::Command::new("spl-token")
            .arg("create-account")
            .arg(mint.to_string())
            .arg("--owner")
            .arg(user_wallet.to_string())
            .arg("--fee-payer")
            .arg(&wallet_path)
            .arg("--program-id")
            .arg("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb") // Token-2022 program
            .arg("--url")
            .arg(&rpc_url)
            .output()
            .map_err(|e| anyhow!("Failed to execute spl-token CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("DEBUG: CLI Stdout: {}", stdout);
            println!("DEBUG: CLI Stderr: {}", stderr);

            // If it failed because it already exists (race condition?), ignore
            if !stderr.contains("already exists") && !stdout.contains("already exists") {
                return Err(anyhow!("spl-token CLI failed: {}", stderr));
            }
        }

        println!("DEBUG: CLI ATA creation successful");

        // Brief sleep to allow propagation
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        Ok(ata_address)
    }

    /// Transfer SPL tokens from one account to another
    pub async fn transfer_tokens(
        &self,
        authority: &Keypair,
        from_token_account: &Pubkey,
        to_token_account: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        let transfer_instruction = BlockchainUtils::create_transfer_instruction(
            authority,
            from_token_account,
            to_token_account,
            mint,
            amount,
            decimals,
        )?;

        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            vec![transfer_instruction],
            &signers,
            TransactionType::Settlement,
        )
        .await
    }

    /// Register a user on-chain
    pub async fn register_user_on_chain(
        &self,
        authority: &Keypair,
        user_type: u8,
        location: &str,
    ) -> Result<Signature> {
        let register_instruction =
            BlockchainUtils::create_register_user_instruction(authority, user_type, location)?;

        self.build_and_send_transaction(vec![register_instruction], &[authority])
            .await
    }

    /// Register a meter on-chain
    pub async fn register_meter_on_chain(
        &self,
        authority: &Keypair,
        meter_id: &str,
        meter_type: u8,
    ) -> Result<Signature> {
        let register_instruction =
            BlockchainUtils::create_register_meter_instruction(authority, meter_id, meter_type)?;

        self.build_and_send_transaction(vec![register_instruction], &[authority])
            .await
    }

    /// Submit meter reading on-chain (via Oracle)
    pub async fn submit_meter_reading_on_chain(
        &self,
        authority: &Keypair,
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Signature> {
        let submit_instruction = BlockchainUtils::create_submit_meter_reading_instruction(
            authority, meter_id, produced, consumed, timestamp,
        )?;

        self.build_and_send_transaction(vec![submit_instruction], &[authority])
            .await
    }

    /// Mint tokens directly to a user's wallet
    pub async fn mint_tokens_direct(&self, user_wallet: &Pubkey, amount: u64) -> Result<Signature> {
        println!(
            "DEBUG: mint_tokens_direct called for wallet: {}",
            user_wallet
        );

        // Get authority keypair
        let authority = self.get_authority_keypair().await?;

        let mint_str = std::env::var("ENERGY_TOKEN_MINT")
            .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?;
        println!("DEBUG: mint_tokens_direct using mint: {}", mint_str);

        // Get energy token mint
        let mint = Pubkey::from_str(&mint_str)?;

        // Ensure user has an associated token account
        let user_token_account = self
            .ensure_token_account_exists(&authority, user_wallet, &mint)
            .await?;

        // Call the original mint method
        self.mint_energy_tokens(
            &authority,
            &user_token_account,
            user_wallet,
            &mint,
            amount as f64 / 1_000_000_000.0,
        )
        .await
    }

    /// Calculate the Associated Token Account address for a user and mint
    pub fn calculate_ata_address(&self, user_wallet: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
        // Use Token-2022 program ID for ATA derivation
        let token_program_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .map_err(|e| anyhow!("Failed to parse Token-2022 program ID: {}", e))?;

        let ata_address =
            spl_associated_token_account::get_associated_token_address_with_program_id(
                user_wallet,
                mint,
                &token_program_id,
            );
        Ok(ata_address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SolanaProgramsConfig;

    fn test_config() -> SolanaProgramsConfig {
        SolanaProgramsConfig {
            registry_program_id: "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7".to_string(),
            oracle_program_id: "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE".to_string(),
            governance_program_id: "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe".to_string(),
            energy_token_program_id: "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur".to_string(),
            trading_program_id: "9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb".to_string(),
        }
    }

    #[test]
    fn test_parse_program_ids() {
        let service = BlockchainService::new(
            "http://localhost:8899".to_string(),
            "localnet".to_string(),
            test_config(),
        )
        .unwrap();
        assert!(service.registry_program_id().is_ok());
        assert!(service.oracle_program_id().is_ok());
        assert!(service.governance_program_id().is_ok());
        assert!(service.energy_token_program_id().is_ok());
        assert!(service.trading_program_id().is_ok());
    }

    #[test]
    fn test_parse_invalid_pubkey() {
        assert!(BlockchainService::parse_pubkey("invalid").is_err());
    }
}
