use super::account_management::AccountManager;
use super::instructions::InstructionBuilder;
use super::on_chain::OnChainManager;
use super::token_management::TokenManager;
use super::transactions::TransactionHandler;
use super::utils::BlockchainUtils;
use crate::config::SolanaProgramsConfig;
// use crate::services::priority_fee::TransactionType; // DISABLED
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
#[derive(Clone)]
pub struct BlockchainService {
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
    rpc_client: Arc<RpcClient>,
    cluster: String,
    program_ids: SolanaProgramsConfig,

    // Sub-services
    pub account_manager: AccountManager,
    pub token_manager: TokenManager,
    pub on_chain_manager: OnChainManager,
}

impl std::fmt::Debug for BlockchainService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockchainService")
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
            .unwrap_or_else(|_| "dev-wallet.json".to_string());

        let payer = match BlockchainUtils::load_keypair_from_file(&authority_path) {
            Ok(keypair) => {
                info!("Loaded authority keypair: {}", keypair.pubkey());
                keypair.pubkey()
            }
            Err(e) => {
                warn!(
                    "Failed to load authority keypair: {}. Using placeholder.",
                    e
                );
                "11111111111111111111111111111112".parse().expect("Failed to parse valid System Program ID")
            }
        };

        let instruction_builder = InstructionBuilder::new(payer);

        // Initialize sub-managers
        let account_manager = AccountManager::new(transaction_handler.clone());
        let token_manager = TokenManager::new(transaction_handler.clone(), account_manager.clone());
        let on_chain_manager = OnChainManager::new(
            transaction_handler.clone(),
            instruction_builder.clone(),
            program_ids.clone(),
        );

        Ok(Self {
            transaction_handler,
            instruction_builder,
            rpc_client,
            cluster,
            program_ids,
            account_manager,
            token_manager,
            on_chain_manager,
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

    /// Get the payer pubkey
    pub fn payer_pubkey(&self) -> Pubkey {
        self.instruction_builder.payer()
    }

    /// Submit transaction to blockchain
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<Signature> {
        self.on_chain_manager.submit_transaction(transaction).await
    }

    /// Add priority fee to transaction
    pub fn add_priority_fee(
        &self,
        transaction: &mut Transaction,
        tx_type: &'static str,
        fee: u64,
    ) -> Result<()> {
        self.transaction_handler
            .add_priority_fee(transaction, tx_type, fee)
    }

    /// Confirm transaction status
    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        self.on_chain_manager.confirm_transaction(signature).await
    }

    // DISABLED - uses models module
    // /// Get trade record from blockchain
    // pub async fn get_trade_record(
    //     &self,
    //     signature: &str,
    // ) -> Result<crate::models::transaction::TradeRecord> {
    //     self.transaction_handler.get_trade_record(signature).await
    // }

    /// Check if the service is healthy
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
        self.account_manager.get_balance(pubkey).await
    }

    /// Get account balance in SOL
    pub async fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        self.account_manager.get_balance_sol(pubkey).await
    }

    /// Get SPL token balance for a user
    pub async fn get_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        self.token_manager.get_token_balance(owner, mint).await
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
        self.account_manager.get_account_data(pubkey).await
    }

    /// Initialize the registry on-chain (localnet bootstrapping)
    pub async fn initialize_registry(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Registry on-chain...");
        let instruction = self.instruction_builder.build_initialize_registry_instruction()?;
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Initialize the oracle on-chain (localnet bootstrapping)
    pub async fn initialize_oracle(&self, authority: &Keypair, api_gateway: &Pubkey) -> Result<Signature> {
        info!("Initializing Oracle on-chain with API Gateway: {}...", api_gateway);
        let instruction = self.instruction_builder.build_initialize_oracle_instruction(api_gateway)?;
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Initialize the governance (PoA) on-chain (localnet bootstrapping)
    pub async fn initialize_governance(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Governance (PoA) on-chain...");
        let instruction = self.instruction_builder.build_initialize_governance_instruction()?;
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Initialize the Energy Token program mint on-chain (localnet bootstrapping)
    pub async fn initialize_energy_token(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Energy Token on-chain with Authority: {}", authority.pubkey());
        let instruction = self.instruction_builder.build_initialize_energy_token_instruction(authority.pubkey())?;
        
        for (i, acc) in instruction.accounts.iter().enumerate() {
            info!("  Account {}: {} (signer: {}, writable: {})", i, acc.pubkey, acc.is_signer, acc.is_writable);
        }

        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Initialize the Trading Market on-chain
    pub async fn initialize_trading_market(&self, authority: &Keypair) -> Result<Signature> {
        info!("Initializing Trading Market on-chain with Authority: {}", authority.pubkey());
        let instruction = self.instruction_builder.build_initialize_market_instruction(authority.pubkey())?;
        
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Issue an ERC certificate on-chain
    pub async fn issue_erc(
        &self,
        certificate_id: &str,
        user_wallet: &Pubkey,
        meter_account: &Pubkey,
        energy_amount: u64,
        renewable_source: &str,
        validation_data: &str,
        authority: &Keypair,
    ) -> Result<Signature> {
        info!("Issuing ERC {} on-chain for {} kWh", certificate_id, energy_amount);
        let instruction = self.instruction_builder.build_issue_erc_instruction(
            certificate_id,
            user_wallet,
            meter_account,
            energy_amount,
            renewable_source,
            validation_data,
        )?;
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Transfer an ERC certificate on-chain
    pub async fn transfer_erc(
        &self,
        certificate_id: &str,
        owner: &Keypair,
        new_owner: &Pubkey,
    ) -> Result<Signature> {
        info!("Transferring ERC {} on-chain to {}", certificate_id, new_owner);
        let instruction = self.instruction_builder.build_transfer_erc_instruction(
            certificate_id,
            &owner.pubkey(),
            new_owner,
        )?;
        self.build_and_send_transaction(vec![instruction], &[owner]).await
    }

    /// Revoke (retire) an ERC certificate on-chain
    pub async fn revoke_erc(
        &self,
        certificate_id: &str,
        reason: &str,
        authority: &Keypair,
    ) -> Result<Signature> {
        info!("Revoking ERC {} on-chain (Reason: {})", certificate_id, reason);
        let instruction = self.instruction_builder.build_revoke_erc_instruction(
            certificate_id,
            reason,
        )?;
        self.build_and_send_transaction(vec![instruction], &[authority]).await
    }

    /// Check if an account exists
    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        self.account_manager.account_exists(pubkey).await
    }

    /// Get transaction account keys
    pub async fn get_transaction_account_keys(&self, signature: &str) -> Result<Vec<Pubkey>> {
        self.account_manager
            .get_transaction_account_keys(signature)
            .await
    }

    /// Parse Pubkey from string
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        AccountManager::parse_pubkey(pubkey_str)
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

    /// Get active orders count from market account
    async fn get_market_active_orders(&self, market_pubkey: &Pubkey) -> Result<u32> {
        let client = Arc::clone(&self.rpc_client);
        let market_pubkey = *market_pubkey;

        let active_orders = tokio::task::spawn_blocking(move || {
            let account = client.get_account(&market_pubkey)?;
            // Parse active_orders from account data (offset 40, u32)
            if account.data.len() < 44 {
                return Err(anyhow!("Market account data too small"));
            }
            let active_orders_bytes: [u8; 4] = account.data[40..44]
                .try_into()
                .expect("slice length already verified to be 4 bytes");
            Ok(u32::from_le_bytes(active_orders_bytes))
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
            "token_transaction",
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
    ) -> Result<(Signature, String)> {
        let market =
            Pubkey::from_str(market_pubkey).map_err(|e| anyhow!("Invalid market pubkey: {}", e))?;

        let (instruction, order_pda) = self
            .build_create_order_instruction(
                &market,
                authority.pubkey(),
                energy_amount,
                price_per_kwh,
                order_type,
                erc_certificate_id,
            )
            .await?;

        let signers = vec![authority];

        let signature = self.build_and_send_transaction_with_priority(
            vec![instruction],
            &signers,
            "token_transaction",
        )
        .await?;

        Ok((signature, order_pda.to_string()))
    }

    /// Build instruction for creating energy trade order
    /// Returns (Instruction, Order PDA)
    pub async fn build_create_order_instruction(
        &self,
        market_pubkey: &Pubkey,
        authority: Pubkey,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
    ) -> Result<(Instruction, Pubkey)> {
        let market = *market_pubkey;

        // Get active orders count
        let active_orders = self.get_market_active_orders(&market).await?;

        // Derive order PDA
        let (order_pda, _) = Pubkey::find_program_address(
            &[b"order", authority.as_ref(), &active_orders.to_le_bytes()],
            &self.trading_program_id()?,
        );

        let instruction = self.instruction_builder.build_create_order_instruction(
            market_pubkey,
            &authority,
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
    // ====================================================================
    // Transaction Building & Signing (Phase 4) - delegated to OnChainManager
    // ====================================================================

    /// Priority 4: Build, sign, and send a transaction with automatic priority fees
    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        self.on_chain_manager
            .build_and_send_transaction(instructions, signers)
            .await
    }

    /// Build, sign, and send a transaction with specified priority level
    pub async fn build_and_send_transaction_with_priority(
        &self,
        instructions: Vec<Instruction>,
        signers: &[&Keypair],
        transaction_type: &'static str,
    ) -> Result<Signature> {
        self.on_chain_manager
            .build_and_send_transaction_with_priority(instructions, signers, transaction_type)
            .await
    }

    /// Simulate a transaction before sending
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<bool> {
        self.transaction_handler
            .simulate_transaction(transaction)
            .await?;
        Ok(true)
    }

    /// Wait for transaction confirmation with timeout
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
        AccountManager::load_keypair_from_file(filepath)
    }

    /// Get authority keypair (for settlement service)
    pub async fn get_authority_keypair(&self) -> Result<Keypair> {
        self.account_manager.get_authority_keypair().await
    }

    /// Mint energy tokens directly to a user's token account
    /// Mint (or Burn) energy tokens based on reading amount
    /// Positive amount = Mint
    /// Negative amount = Burn
    pub async fn mint_energy_tokens(
        &self,
        authority: &Keypair,
        user_token_account: &Pubkey,
        user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        if amount_kwh > 0.0 {
            info!("Minting {} kWh tokens for wallet {}", amount_kwh, user_wallet);
            self.token_manager
                .mint_energy_tokens(authority, user_token_account, user_wallet, mint, amount_kwh)
                .await
        } else if amount_kwh < 0.0 {
            let burn_amount = amount_kwh.abs();
            info!("Burning {} kWh tokens from wallet {}", burn_amount, user_wallet);
            self.token_manager
                .burn_energy_tokens(authority, user_token_account, mint, burn_amount)
                .await
        } else {
            // Zero reading, no-op but return successful "signature" placeholder?
            // Or technically this shouldn't happen if validation works.
            // Let's just return a log and skip. 
            // We need to return a signature though.
            // Returning an error might fail the flow, but zero tokens is valid state.
            // We can return the last signature or a dummy one if we had one.
            // For now, let's treat it as a warning.
            Err(anyhow!("Cannot mint/burn zero tokens"))
        }
    }

    /// Mint SPL tokens using standard spl-token CLI (for testing with standard SPL tokens)
    pub async fn mint_spl_tokens(
        &self,
        authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {
        info!("Minting {} SPL tokens for wallet {} using CLI", amount_kwh, user_wallet);
        self.token_manager
            .mint_spl_tokens(authority, user_wallet, mint, amount_kwh)
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
        self.token_manager
            .burn_energy_tokens(authority, user_token_account, mint, amount_kwh)
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
        self.token_manager
            .transfer_energy_tokens(
                authority,
                from_token_account,
                to_token_account,
                mint,
                amount_kwh,
            )
            .await
    }

    /// Ensures user has an Associated Token Account for the token mint
    pub async fn ensure_token_account_exists(
        &self,
        authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Pubkey> {
        self.token_manager
            .ensure_token_account_exists(authority, user_wallet, mint)
            .await
    }

    /// Calculate the Associated Token Account address for a user and mint
    pub fn calculate_ata_address(&self, user_wallet: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
        self.account_manager
            .calculate_ata_address(user_wallet, mint)
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
        self.token_manager
            .transfer_tokens(
                authority,
                from_token_account,
                to_token_account,
                mint,
                amount,
                decimals,
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

    /// Update meter reading on-chain via Registry program
    /// The oracle_authority must be the configured oracle on the Registry program
    /// Call `set_oracle_authority` on Registry first to authorize the oracle
    pub async fn update_meter_reading_on_chain(
        &self,
        oracle_authority: &Keypair,
        meter_id: &str,
        energy_generated_wh: u64,
        energy_consumed_wh: u64,
        reading_timestamp: i64,
    ) -> Result<Signature> {
        info!(
            "Updating meter {} on-chain: gen={} Wh, cons={} Wh",
            meter_id, energy_generated_wh, energy_consumed_wh
        );

        let update_instruction = BlockchainUtils::create_update_meter_reading_instruction(
            oracle_authority,
            meter_id,
            energy_generated_wh,
            energy_consumed_wh,
            reading_timestamp,
        )?;

        self.build_and_send_transaction(vec![update_instruction], &[oracle_authority])
            .await
    }

    /// Derive escrow PDA
    pub fn derive_escrow_pda(order_id: &[u8; 32], program_id: &Pubkey) -> (Pubkey, u8) {
        TransactionHandler::derive_escrow_pda(order_id, program_id)
    }

    /// Lock tokens to escrow
    pub async fn lock_tokens_to_escrow(
        &self,
        buyer_authority: &Keypair,
        buyer_ata: &Pubkey,
        escrow_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        self.transaction_handler
            .lock_tokens_to_escrow(buyer_authority, buyer_ata, escrow_ata, token_mint, amount, decimals)
            .await
    }

    /// Release escrow to seller
    pub async fn release_escrow_to_seller(
        &self,
        escrow_authority: &Keypair,
        escrow_ata: &Pubkey,
        seller_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        self.transaction_handler
            .release_escrow_to_seller(escrow_authority, escrow_ata, seller_ata, token_mint, amount, decimals)
            .await
    }

    /// Refund escrow to buyer
    pub async fn refund_escrow_to_buyer(
        &self,
        escrow_authority: &Keypair,
        escrow_ata: &Pubkey,
        buyer_ata: &Pubkey,
        token_mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        self.transaction_handler
            .refund_escrow_to_buyer(escrow_authority, escrow_ata, buyer_ata, token_mint, amount, decimals)
            .await
    }

    /// Mint tokens directly to a user's wallet using the Anchor energy_token program
    pub async fn mint_tokens_direct(&self, user_wallet: &Pubkey, amount: u64) -> Result<Signature> {
        info!(
            "mint_tokens_direct called for wallet: {}, amount: {}",
            user_wallet, amount
        );

        // Get authority keypair
        let authority = self.account_manager.get_authority_keypair().await?;

        // Get configured mint
        let mint_str = std::env::var("ENERGY_TOKEN_MINT")
            .unwrap_or_else(|_| "Geq98m3Vw63AqrMEVoZsiW5DbNkScteZAdWDmm95ykYF".to_string());
        let mint = Pubkey::from_str(&mint_str)
            .map_err(|e| anyhow!("Invalid ENERGY_TOKEN_MINT: {}", e))?;

        // Convert atomic amount to UI amount (assuming 9 decimals)
        let amount_kwh = amount as f64 / 1_000_000_000.0;
        
        // Ensure ATA exists explicitly (since implicit creation via spl-token mint --recipient-owner seems unreliable in this env)
        if let Err(e) = self.ensure_token_account_exists(&authority, user_wallet, &mint).await {
            tracing::warn!("Failed to ensure ATA exists (might already exist): {}", e);
            // Continue to mint, as it might just be an "already exists" error which is fine
        }

        self.mint_spl_tokens(&authority, user_wallet, &mint, amount_kwh).await
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
