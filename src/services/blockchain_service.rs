use crate::services::blockchain_instructions::InstructionBuilder;
use crate::services::blockchain_transactions::TransactionHandler;
use crate::services::blockchain_utils::BlockchainUtils;
use crate::services::priority_fee_service::TransactionType;
use anyhow::{Result, anyhow};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
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
    cluster: String,
}

impl BlockchainService {
    /// Create a new blockchain service
    pub fn new(rpc_url: String, cluster: String) -> Result<Self> {
        info!("Initializing blockchain service for cluster: {}", cluster);

        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let transaction_handler = TransactionHandler::new(Arc::clone(&rpc_client));

        // Load payer from secure storage or use placeholder for development
        let payer = std::env::var("PAYER_PRIVATE_KEY")
            .ok()
            .and_then(|key| key.parse().ok())
            .unwrap_or_else(|| {
                warn!("Using placeholder payer key - set PAYER_PRIVATE_KEY env var for production");
                "11111111111111111111111111111112".parse().unwrap()
            });

        let instruction_builder = InstructionBuilder::new(payer);

        Ok(Self {
            transaction_handler,
            instruction_builder,
            cluster,
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
        "11111111111111111111111111111111112".parse().unwrap()
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

    /// Parse Pubkey from string
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        BlockchainUtils::parse_pubkey(pubkey_str)
    }

    /// Get Registry program ID
    pub fn registry_program_id() -> Result<Pubkey> {
        // TODO: Implement program_ids module or load from config
        Err(anyhow!("Program IDs not configured"))
    }

    /// Get Oracle program ID
    pub fn oracle_program_id() -> Result<Pubkey> {
        // TODO: Implement program_ids module or load from config
        Err(anyhow!("Program IDs not configured"))
    }

    /// Get Governance program ID
    pub fn governance_program_id() -> Result<Pubkey> {
        // TODO: Implement program_ids module or load from config
        Err(anyhow!("Program IDs not configured"))
    }

    /// Get Energy Token program ID
    pub fn energy_token_program_id() -> Result<Pubkey> {
        // TODO: Implement program_ids module or load from config
        Err(anyhow!("Program IDs not configured"))
    }

    /// Get Trading program ID
    pub fn trading_program_id() -> Result<Pubkey> {
        // TODO: Implement program_ids module or load from config
        Err(anyhow!("Program IDs not configured"))
    }

    // ====================================================================
    // Instruction Building Methods (delegated to InstructionBuilder)
    // ====================================================================

    /// Build instruction for creating energy trade order
    pub fn build_create_order_instruction(
        &self,
        market_pubkey: &str,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: &str,
        erc_certificate_id: Option<&str>,
    ) -> Result<Instruction> {
        self.instruction_builder.build_create_order_instruction(
            market_pubkey,
            energy_amount,
            price_per_kwh,
            order_type,
            erc_certificate_id,
        )
    }

    /// Build instruction for matching orders
    pub fn build_match_orders_instruction(
        &self,
        market_pubkey: &str,
        buy_order_pubkey: &str,
        sell_order_pubkey: &str,
        match_amount: u64,
    ) -> Result<Instruction> {
        self.instruction_builder.build_match_orders_instruction(
            market_pubkey,
            buy_order_pubkey,
            sell_order_pubkey,
            match_amount,
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
        let mint_instruction = BlockchainUtils::create_mint_instruction(
            authority,
            user_token_account,
            user_wallet,
            mint,
            amount_kwh,
        )?;

        let signers = vec![authority];
        self.build_and_send_transaction_with_priority(
            vec![mint_instruction],
            &signers,
            TransactionType::TokenMinting,
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
        // Check if account exists first
        let ata_address = self.calculate_ata_address(user_wallet, mint)?;
        if self.account_exists(&ata_address).await? {
            info!("ATA already exists: {}", ata_address);
            return Ok(ata_address);
        }

        // Create ATA instruction
        let create_ata_instruction =
            BlockchainUtils::create_ata_instruction(authority, user_wallet, mint)?;

        // Submit transaction
        let signature = self
            .build_and_send_transaction(vec![create_ata_instruction], &[authority])
            .await?;

        info!("ATA created. Signature: {}", signature);

        // Wait for confirmation
        self.wait_for_confirmation(&signature, 30).await?;

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
        // Get authority keypair
        let authority = self.get_authority_keypair().await?;

        // Get energy token mint
        let mint = Pubkey::from_str(
            &std::env::var("ENERGY_TOKEN_MINT")
                .map_err(|e| anyhow!("ENERGY_TOKEN_MINT not set: {}", e))?,
        )?;

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

    // Helper method to calculate ATA address
    fn calculate_ata_address(&self, user_wallet: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
        let ata_program_id = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")?;
        let token_program_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;

        let (ata_address, _bump) = Pubkey::find_program_address(
            &[
                user_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            &ata_program_id,
        );

        Ok(ata_address)
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
