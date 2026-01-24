use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use crate::services::blockchain::transactions::TransactionHandler;

/// Manages account-related blockchain queries
#[derive(Clone, Debug)]
pub struct AccountManager {
    rpc_client: Arc<RpcClient>,
    transaction_handler: TransactionHandler,
}

impl AccountManager {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let transaction_handler = TransactionHandler::new(rpc_client.clone());
        Self {
            rpc_client,
            transaction_handler,
        }
    }

    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        self.transaction_handler.confirm_transaction(&pubkey.to_string()).await
    }

    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<solana_sdk::account::Account> {
        let conn = self.rpc_client.clone();
        let pubkey = *pubkey;
        tokio::task::spawn_blocking(move || {
            conn.get_account(&pubkey).map_err(|e| anyhow!("Failed to get account: {}", e))
        })
        .await?
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self.get_account(pubkey).await?;
        Ok(account.data)
    }

    pub fn calculate_ata_address(&self, wallet: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
        let token_program_id = spl_token::id(); // Use standard spl_token for derivation if needed or Token-2022
        // Actually, the app seems to use Token-2022 for energy tokens.
        let token_2022_id = solana_sdk::pubkey::Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
            .map_err(|e| anyhow!("Invalid Token-2022 ID: {}", e))?;
            
        Ok(spl_associated_token_account::get_associated_token_address_with_program_id(
            wallet, mint, &token_2022_id
        ))
    }

    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        pubkey_str.parse().map_err(|e| anyhow!("Invalid pubkey '{}': {}", pubkey_str, e))
    }

    pub async fn get_transaction_account_keys(&self, _signature: &str) -> Result<Vec<Pubkey>> {
        // Mock or implement if needed
        Ok(vec![])
    }
}
