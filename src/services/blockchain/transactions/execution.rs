use anyhow::{anyhow, Result};
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use super::pool::ConnectionPool;
use super::signing::SigningManager;

pub struct ExecutionManager;

impl ExecutionManager {
    pub async fn submit_with_retry(
        pool: &ConnectionPool,
        mut transaction: Transaction,
    ) -> Result<Signature> {
        let mut attempts = 0;
        let max_retries = 5;
        let base_delay_ms = 500u64;
        let max_delay_ms = 30_000u64;

        loop {
            attempts += 1;
            
            let conn = pool.get_connection().await;
            let recent_blockhash = conn.get_latest_blockhash().map_err(|e| anyhow!("Retry blockhash failed: {}", e))?;
            transaction.message.recent_blockhash = recent_blockhash;
            
            let payer = SigningManager::get_payer_keypair().await?;
            transaction.try_sign(&[&payer], recent_blockhash).map_err(|e| anyhow!("Retry sign failed: {}", e))?;

            match conn.send_and_confirm_transaction(&transaction) {
                Ok(sig) => {
                    info!("Transaction submitted successfully on attempt {}", attempts);
                    pool.return_connection(conn).await;
                    return Ok(sig);
                }
                Err(e) => {
                    pool.return_connection(conn).await;
                    let err_str = e.to_string();
                    error!("Attempt {} failed: {}", attempts, err_str);

                    if err_str.contains("insufficient funds") || attempts >= max_retries {
                        return Err(anyhow!("Transaction failed: {}", e));
                    }
                }
            }

            let delay = Duration::from_millis(capped_backoff(attempts, base_delay_ms, max_delay_ms));
            tokio::time::sleep(delay).await;
        }
    }
}

fn capped_backoff(attempts: u32, base: u64, max: u64) -> u64 {
    let exp = base.saturating_mul(1u64 << (attempts - 1));
    let jitter = rand::random::<u64>() % (exp / 4 + 1);
    exp.min(max) + jitter
}
