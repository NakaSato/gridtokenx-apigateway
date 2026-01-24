use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug};

pub struct QueryManager {
    recent_blockhash: Arc<RwLock<Option<solana_sdk::hash::Hash>>>,
}

impl QueryManager {
    pub fn new() -> Self {
        Self {
            recent_blockhash: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_recent_blockhash(&self, rpc_client: Arc<RpcClient>) -> Result<solana_sdk::hash::Hash> {
        {
            let cache = self.recent_blockhash.read().await;
            if let Some(blockhash) = *cache {
                return Ok(blockhash);
            }
        }

        let blockhash = rpc_client.get_latest_blockhash().map_err(|e| anyhow!("Failed to get blockhash: {}", e))?;
        {
            let mut cache = self.recent_blockhash.write().await;
            *cache = Some(blockhash);
        }
        Ok(blockhash)
    }

    pub async fn get_balance(&self, rpc_client: Arc<RpcClient>, pubkey: &Pubkey) -> Result<u64> {
        rpc_client.get_balance(pubkey).map_err(|e| anyhow!("Failed to get balance: {}", e))
    }

    pub async fn get_token_account_balance(&self, rpc_client: Arc<RpcClient>, token_account: &Pubkey) -> Result<u64> {
        let balance = rpc_client.get_token_account_balance(token_account).map_err(|e| anyhow!("Failed to get token balance: {}", e))?;
        balance.amount.parse::<u64>().map_err(|e| anyhow!("Failed to parse token amount: {}", e))
    }

    pub async fn get_account_data(&self, rpc_client: Arc<RpcClient>, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = rpc_client.get_account(pubkey).map_err(|e| anyhow!("Failed to get account: {}", e))?;
        Ok(account.data)
    }
}
