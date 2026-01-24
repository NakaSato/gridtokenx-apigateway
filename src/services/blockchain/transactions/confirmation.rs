use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Processed,
    Confirmed(u64), // number of confirmations
    Finalized,
    Failed(String),
}

pub struct ConfirmationManager;

impl ConfirmationManager {
    pub async fn confirm_transaction(rpc_client: Arc<RpcClient>, signature: &str) -> Result<bool> {
        let sig = Signature::from_str(signature).map_err(|e| anyhow!("Invalid signature: {}", e))?;
        let status = rpc_client.get_signature_status(&sig).map_err(|e| anyhow!("Failed to get signature status: {}", e))?;
        Ok(status.is_some())
    }

    pub async fn confirm_transaction_with_polling(
        rpc_client: Arc<RpcClient>,
        signature: &Signature,
        timeout_secs: u64,
        poll_interval_ms: u64,
    ) -> Result<TransactionStatus> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let poll_interval = Duration::from_millis(poll_interval_ms);
        let mut last_status = TransactionStatus::Pending;

        loop {
            if start.elapsed() >= timeout {
                return Ok(TransactionStatus::Pending);
            }

            let status = Self::get_transaction_status(rpc_client.clone(), signature).await?;
            if status != last_status {
                info!("Transaction {} status: {:?}", signature, status);
                last_status = status.clone();
            }

            match status {
                TransactionStatus::Finalized => return Ok(TransactionStatus::Finalized),
                TransactionStatus::Confirmed(count) if count >= 32 => return Ok(TransactionStatus::Finalized),
                TransactionStatus::Failed(err) => return Ok(TransactionStatus::Failed(err)),
                _ => {}
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    pub async fn get_transaction_status(rpc_client: Arc<RpcClient>, signature: &Signature) -> Result<TransactionStatus> {
        let status = rpc_client.get_signature_status(signature).map_err(|e| anyhow!("Failed to get signature status: {}", e))?;

        match status {
            None => Ok(TransactionStatus::Pending),
            Some(result) => match result {
                Ok(_) => {
                    if let Some(confirmations) = rpc_client.get_signature_statuses(&[*signature])?.value[0].as_ref() {
                        if confirmations.confirmation_status.is_some() {
                            use solana_transaction_status::TransactionConfirmationStatus;
                            match confirmations.confirmation_status.as_ref().unwrap() {
                                TransactionConfirmationStatus::Finalized => Ok(TransactionStatus::Finalized),
                                TransactionConfirmationStatus::Confirmed => Ok(TransactionStatus::Confirmed(confirmations.confirmations.unwrap_or(1) as u64)),
                                TransactionConfirmationStatus::Processed => Ok(TransactionStatus::Processed),
                            }
                        } else {
                             Ok(TransactionStatus::Processed)
                        }
                    } else {
                        Ok(TransactionStatus::Processed)
                    }
                }
                Err(err) => Ok(TransactionStatus::Failed(err.to_string())),
            },
        }
    }
}
