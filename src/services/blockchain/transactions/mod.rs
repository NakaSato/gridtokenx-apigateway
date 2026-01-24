use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::sync::Arc;
use tracing::info;

pub mod pool;
pub mod signing;
pub mod validation;
pub mod execution;
pub mod queries;
pub mod confirmation;

pub use pool::ConnectionPool;
pub use signing::SigningManager;
pub use validation::ValidationManager;
pub use execution::ExecutionManager;
pub use queries::QueryManager;
pub use confirmation::{ConfirmationManager, TransactionStatus};

// Re-exports for backward compatibility with service.rs
pub use confirmation::TransactionStatus as TransactionStatusEnum;
pub use confirmation::TransactionStatus as SolBalanceCheck; // Fake alias for compatibility if needed
pub use confirmation::TransactionStatus as FeeEstimate; // Fake alias

#[derive(Clone)]
pub struct TransactionHandler {
    pool: Arc<ConnectionPool>,
    queries: Arc<QueryManager>,
}

impl std::fmt::Debug for TransactionHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionHandler")
            .field("rpc_url", &self.pool.client().url())
            .finish()
    }
}

impl TransactionHandler {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        info!("Initializing modularized transaction handler");
        Self {
            pool: Arc::new(ConnectionPool::new(rpc_client)),
            queries: Arc::new(QueryManager::new()),
        }
    }

    pub async fn submit_transaction(&self, mut transaction: Transaction) -> Result<Signature> {
        let conn = self.pool.get_connection().await;
        let recent_blockhash = self.queries.get_recent_blockhash(conn.clone()).await?;
        self.pool.return_connection(conn).await;
        
        transaction.message.recent_blockhash = recent_blockhash;

        ValidationManager::validate_transaction(&transaction)?;
        
        let payer = SigningManager::get_payer_keypair().await?;
        let signature = SigningManager::sign_transaction(&mut transaction, &payer, recent_blockhash).await?;

        ExecutionManager::submit_with_retry(&self.pool, transaction).await
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let conn = self.pool.get_connection().await;
        let res = self.queries.get_balance(conn.clone(), pubkey).await;
        self.pool.return_connection(conn).await;
        res
    }

    pub async fn get_token_account_balance(&self, token_account: &Pubkey) -> Result<u64> {
        let conn = self.pool.get_connection().await;
        let res = self.queries.get_token_account_balance(conn.clone(), token_account).await;
        self.pool.return_connection(conn).await;
        res
    }

    pub async fn confirm_transaction(&self, signature: &str) -> Result<bool> {
        ConfirmationManager::confirm_transaction(self.pool.arc_client(), signature).await
    }

    pub async fn confirm_transaction_with_polling(&self, signature: &Signature, timeout: u64, interval: u64) -> Result<TransactionStatus> {
        ConfirmationManager::confirm_transaction_with_polling(self.pool.arc_client(), signature, timeout, interval).await
    }

    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<solana_sdk::account::Account> {
        let conn = self.pool.get_connection().await;
        let res = conn.get_account(pubkey).map_err(|e| anyhow::anyhow!("Failed to get account: {}", e));
        self.pool.return_connection(conn).await;
        res
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let conn = self.pool.get_connection().await;
        let res = self.queries.get_account_data(conn.clone(), pubkey).await;
        self.pool.return_connection(conn).await;
        res
    }

    pub async fn build_and_send_transaction(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        signers: &[&Keypair],
    ) -> Result<Signature> {
        let conn = self.pool.get_connection().await;
        let recent_blockhash = self.queries.get_recent_blockhash(conn.clone()).await?;
        self.pool.return_connection(conn).await;

        let mut transaction = Transaction::new_with_payer(&instructions, Some(&signers[0].pubkey()));
        transaction.sign(signers, recent_blockhash);

        ExecutionManager::submit_with_retry(&self.pool, transaction).await
    }

    pub async fn build_and_send_transaction_with_priority(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        signers: &[&Keypair],
        _transaction_type: &'static str,
    ) -> Result<Signature> {
        // For now, just call the regular method
        self.build_and_send_transaction(instructions, signers).await
    }

    pub fn client(&self) -> &RpcClient {
        self.pool.client()
    }
}
