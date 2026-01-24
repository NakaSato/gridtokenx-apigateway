use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::transaction::Transaction;
use tracing::{debug, warn};
use std::sync::Arc;

pub struct ValidationManager;

impl ValidationManager {
    pub async fn simulate_transaction(
        rpc_client: Arc<RpcClient>,
        transaction: &Transaction,
    ) -> Result<()> {
        debug!(
            "Simulating transaction with {} instructions",
            transaction.message.instructions.len()
        );

        let config = solana_client::rpc_config::RpcSimulateTransactionConfig {
            sig_verify: true,
            replace_recent_blockhash: true,
            ..Default::default()
        };

        let simulation = rpc_client
            .simulate_transaction_with_config(transaction, config)
            .map_err(|e| anyhow!("Transaction simulation failed: {}", e))?;

        if let Some(err) = simulation.value.err {
            warn!("Transaction simulation errors: {:?}", err);
            return Err(anyhow!(
                "Transaction simulation validation failed: {:?}",
                err
            ));
        }

        if let Some(logs) = &simulation.value.logs {
            for log in logs {
                debug!("Simulation log: {}", log);
            }
        }

        debug!("Transaction simulation completed successfully");
        Ok(())
    }

    pub fn validate_transaction(transaction: &Transaction) -> Result<()> {
        if transaction.message.instructions.is_empty() {
            return Err(anyhow!("Transaction cannot be empty"));
        }

        for (i, instruction) in transaction.message.instructions.iter().enumerate() {
            if instruction.data.is_empty() {
                return Err(anyhow!("Instruction {} cannot be empty", i));
            }
        }

        debug!("Transaction validation passed");
        Ok(())
    }
}
