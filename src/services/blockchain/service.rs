use super::account_management::AccountManager;
use super::instructions::InstructionBuilder;
use super::on_chain::OnChainManager;
use super::token_management::TokenManager as LegacyTokenManager;
use super::transactions::TransactionHandler;
use super::utils::BlockchainUtils;
use crate::config::SolanaProgramsConfig;
use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

/// Blockchain service for interacting with Solana programs
#[derive(Clone)]
pub struct BlockchainService {
    rpc_client: Arc<RpcClient>,
    cluster: String,
    program_ids: SolanaProgramsConfig,
    pub account_manager: AccountManager,
    on_chain_manager: OnChainManager,
    token_manager: LegacyTokenManager,
    transaction_handler: TransactionHandler,
    instruction_builder: InstructionBuilder,
}

impl std::fmt::Debug for BlockchainService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockchainService")
            .field("rpc_url", &self.rpc_client.url())
            .field("cluster", &self.cluster)
            .finish()
    }
}

impl BlockchainService {
    pub fn new(
        rpc_url: String,
        cluster: String,
        program_ids: SolanaProgramsConfig,
    ) -> Result<Self> {
        info!("Initializing BlockchainService for cluster: {}", cluster);
        
        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let transaction_handler = TransactionHandler::new(rpc_client.clone());
        let account_manager = AccountManager::new(rpc_client.clone());
        
        let payer_pubkey = match std::env::var("PAYER_PUBKEY") {
            Ok(pk) => Pubkey::from_str(&pk).unwrap_or(Pubkey::default()),
            Err(_) => Pubkey::default(),
        };
        
        let instruction_builder = InstructionBuilder::new(payer_pubkey);
        let token_manager = LegacyTokenManager::new(
            transaction_handler.clone(),
            account_manager.clone(),
        );

        let on_chain_manager = OnChainManager::new(
            transaction_handler.clone(),
            instruction_builder.clone(),
            program_ids.clone(),
        );

        Ok(Self {
            rpc_client,
            cluster,
            program_ids,
            account_manager,
            on_chain_manager,
            token_manager,
            transaction_handler,
            instruction_builder,
        })
    }

    pub fn client(&self) -> &RpcClient {
        &self.rpc_client
    }

    pub fn payer_pubkey(&self) -> Pubkey {
        self.instruction_builder.payer()
    }

    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<Signature> {
        self.transaction_handler.submit_transaction(transaction).await
    }

    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        self.transaction_handler.confirm_transaction(signature).await
    }

    // --- Core On-Chain Delegation ---

    pub async fn initialize_registry(&self, authority: &Keypair) -> Result<Signature> {
        self.on_chain_manager.registry.initialize_registry(authority).await
    }

    pub async fn initialize_oracle(&self, authority: &Keypair, api_gateway: &Pubkey) -> Result<Signature> {
        self.on_chain_manager.registry.initialize_oracle(authority, api_gateway).await
    }

    pub async fn initialize_governance(&self, authority: &Keypair) -> Result<Signature> {
        self.on_chain_manager.governance.initialize_governance(authority).await
    }

    pub async fn initialize_energy_token(&self, authority: &Keypair) -> Result<Signature> {
        self.on_chain_manager.tokens.initialize_token(authority).await
    }

    pub async fn initialize_trading_market(&self, authority: &Keypair) -> Result<Signature> {
        self.on_chain_manager.trading.initialize_market(authority).await
    }

    pub async fn derive_order_pda(&self, authority: &Pubkey, market_address: &Pubkey) -> Result<Pubkey> {
        self.on_chain_manager.trading.derive_order_pda(authority, market_address).await
    }

    pub async fn execute_create_order(&self, authority: &Keypair, market_address: &Pubkey, side: u8, quantity: u64, price: u64) -> Result<(Signature, String)> {
        self.on_chain_manager.trading.execute_create_order(authority, market_address, side, quantity, price).await
    }

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
        self.on_chain_manager.governance.issue_erc(
            authority, certificate_id, user_wallet, meter_account, energy_amount, renewable_source, validation_data
        ).await
    }

    pub async fn transfer_erc(
        &self,
        certificate_id: &str,
        owner: &Keypair,
        new_owner: &Pubkey,
    ) -> Result<Signature> {
        self.on_chain_manager.governance.transfer_erc(owner, certificate_id, new_owner).await
    }

    pub async fn mint_tokens(&self, authority: &Keypair, recipient: &str, amount: u64) -> Result<Signature> {
        self.on_chain_manager.tokens.mint_tokens(authority, recipient, amount).await
    }

    pub async fn register_user_on_chain(&self, user_authority: &Keypair, user_type: u8, location: &str) -> Result<Signature> {
        self.on_chain_manager.registry.register_user_on_chain(user_authority, user_type, location).await
    }

    pub async fn execute_atomic_settlement(&self, market_authority: &Keypair, settlement_ix: solana_sdk::instruction::Instruction) -> Result<Signature> {
        self.on_chain_manager.trading.execute_settlement(market_authority, settlement_ix).await
    }

    pub async fn initiate_bridge_transfer(
        &self,
        authority: &Keypair,
        market: &Pubkey,
        sell_order: &Pubkey,
        buyer_wallet: &Pubkey,
        amount: u64,
        target_chain: &str,
    ) -> Result<Signature> {
        // Mock bridge for now or delegate if implemented
        info!("Initiating bridge transfer to {}...", target_chain);
        Ok(Signature::default())
    }

    pub async fn complete_bridge_transfer(
        &self,
        authority: &Keypair,
        bridge_id: &str,
        recipient: &Pubkey,
        amount: u64,
    ) -> Result<Signature> {
        info!("Complecing bridge transfer {}...", bridge_id);
        Ok(Signature::default())
    }

    pub async fn get_authority_keypair(&self) -> Result<Keypair> {
        use super::transactions::SigningManager;
        SigningManager::get_payer_keypair().await
    }

    pub async fn mint_tokens_direct(&self, user_wallet: &Pubkey, amount_kwh: f64) -> Result<Signature> {
        let authority_path = std::env::var("AUTHORITY_WALLET_PATH").unwrap_or_else(|_| "dev-wallet.json".to_string());
        let authority = BlockchainUtils::load_keypair_from_file(&authority_path)?;
        let mint = Pubkey::from_str(&std::env::var("ENERGY_TOKEN_MINT")?)?;
        self.token_manager.mint_energy_tokens(&authority, user_wallet, user_wallet, &mint, amount_kwh).await
    }

    /// Handles signature expected by minting handlers: authority, user_token_account, wallet_pubkey, token_mint, amount_kwh
    pub async fn mint_energy_tokens(
        &self, 
        authority: &Keypair, 
        _user_token_account: &Pubkey, 
        user_wallet: &Pubkey, 
        _token_mint: &Pubkey, 
        amount_kwh: f64
    ) -> Result<Signature> {
        let mint = Pubkey::from_str(&std::env::var("ENERGY_TOKEN_MINT")?)?;
        self.token_manager.mint_energy_tokens(authority, user_wallet, user_wallet, &mint, amount_kwh).await
    }

    pub async fn mint_spl_tokens(&self, authority: &Keypair, user_wallet: &Pubkey, mint: &Pubkey, amount_kwh: f64) -> Result<Signature> {
        self.token_manager.mint_spl_tokens(authority, user_wallet, mint, amount_kwh).await
    }

    pub async fn burn_energy_tokens(&self, authority: &Keypair, user_token_account: &Pubkey, mint: &Pubkey, amount_kwh: f64) -> Result<Signature> {
        self.token_manager.burn_energy_tokens(authority, user_token_account, mint, amount_kwh).await
    }

    /// Handles signature expected by meter handlers: authority, meter_id, produced, consumed, timestamp
    pub async fn update_meter_reading_on_chain(
        &self,
        authority: &Keypair,
        meter_id: &str,
        produced: u64,
        consumed: u64,
        timestamp: i64,
    ) -> Result<Signature> {
        self.on_chain_manager.registry.update_meter_reading(authority, meter_id, produced, consumed, timestamp).await
    }

    pub async fn ensure_token_account_exists(&self, authority: &Keypair, user_wallet: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
        self.token_manager.ensure_token_account_exists(authority, user_wallet, mint).await
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        self.account_manager.get_account_data(pubkey).await
    }

    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        BlockchainUtils::parse_pubkey(pubkey_str)
    }

    // --- Utility Methods ---

    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        self.account_manager.account_exists(pubkey).await
    }

    pub fn registry_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.registry_program_id).map_err(|e| anyhow!("Invalid Registry ID: {}", e))
    }

    pub fn trading_program_id(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.program_ids.trading_program_id).map_err(|e| anyhow!("Invalid Trading ID: {}", e))
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.transaction_handler.get_balance(pubkey).await
    }

    pub async fn get_balance_sol(&self, pubkey: &Pubkey) -> Result<f64> {
        let lamports = self.get_balance(pubkey).await?;
        Ok(lamports as f64 / 1_000_000_000.0)
    }

    pub async fn get_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        self.token_manager.get_token_balance(owner, mint).await
    }

    pub async fn transfer_tokens(
        &self,
        authority: &Keypair,
        from: &Pubkey,
        to: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Signature> {
        self.token_manager.transfer_tokens(authority, from, to, mint, amount, decimals).await
    }

    pub async fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        self.rpc_client.request_airdrop(pubkey, lamports).map_err(|e| anyhow!("Airdrop failed: {}", e))
    }

    pub async fn get_slot(&self) -> Result<u64> {
        self.rpc_client.get_slot().map_err(|e| anyhow!("Failed to get slot: {}", e))
    }

    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.rpc_client.get_latest_blockhash().map_err(|e| anyhow!("Failed to get blockhash: {}", e))
    }
}
