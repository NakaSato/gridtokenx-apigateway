use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature};
use std::str::FromStr;
use std::time::Duration; // Added Duration
use tracing::info;

use crate::services::blockchain::account_management::AccountManager; // Dependency
use crate::services::blockchain::transactions::TransactionHandler;
use crate::services::blockchain::utils::BlockchainUtils;
// use crate::services::priority_fee::TransactionType; // DISABLED

/// Manages Token operations (mint, burn, transfer)
#[derive(Clone, Debug)]
pub struct TokenManager {
    transaction_handler: TransactionHandler,
    account_manager: AccountManager,
}

impl TokenManager {
    pub fn new(transaction_handler: TransactionHandler, account_manager: AccountManager) -> Self {
        Self {
            transaction_handler,
            account_manager,
        }
    }

    /// Get SPL token balance for a user
    pub async fn get_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        let ata_address = self.account_manager.calculate_ata_address(owner, mint)?;

        if !self.account_manager.account_exists(&ata_address).await? {
            return Ok(0);
        }

        self.transaction_handler
            .get_token_account_balance(&ata_address)
            .await
    }

    /// Ensures user has an Associated Token Account for the token mint
    pub async fn ensure_token_account_exists(
        &self,
        _authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Pubkey> {
        let ata_address = self
            .account_manager
            .calculate_ata_address(user_wallet, mint)?;

        // Check existence via AccountManager logic (replicated or delegated?)
        // The original code had specific debug prints and logic.
        // It used `transaction_handler.get_account` and checked owner.

        // We can reuse account_manager checks or call transaction_handler directly since we have it.
        match self.transaction_handler.get_account(&ata_address).await {
            Ok(account) => {
                let token_2022_id = solana_sdk::pubkey::Pubkey::from_str(
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
                )
                .expect("hardcoded Token-2022 program ID is invalid");
                if account.owner == token_2022_id || account.owner == spl_token::id() {
                    return Ok(ata_address);
                }
            }
            Err(_) => {}
        }

        // Fallback balance check
        if self
            .transaction_handler
            .get_token_account_balance(&ata_address)
            .await
            .is_ok()
        {
            return Ok(ata_address);
        }

        // Create ATA via CLI (as per original logic - using spl-token CLI)
        let wallet_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "dev-wallet.json".to_string());
        let rpc_url =
            std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://localhost:8899".to_string());

        let output = std::process::Command::new("spl-token")
            .arg("create-account")
            .arg(mint.to_string())
            .arg("--owner")
            .arg(user_wallet.to_string())
            .arg("--fee-payer")
            .arg(&wallet_path)
            .arg("--url")
            .arg(&rpc_url)
            .output()
            .map_err(|e| anyhow!("Failed to execute spl-token CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stderr.contains("already exists") && !stdout.contains("already exists") {
                return Err(anyhow!("spl-token CLI failed: {}", stderr));
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(ata_address)
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
        if !self
            .account_manager
            .account_exists(user_token_account)
            .await?
        {
            info!("ATA {} does not exist, creating it...", user_token_account);
            let create_ata_ix =
                BlockchainUtils::create_ata_instruction(authority, user_wallet, mint)?;
            instructions.push(create_ata_ix);
        }

        let mint_instruction = BlockchainUtils::create_spl_mint_instruction(
            authority,
            user_token_account,
            mint,
            amount_kwh,
        )?;
        instructions.push(mint_instruction);

        let signers = vec![authority];
        self.transaction_handler
            .build_and_send_transaction_with_priority(
                instructions,
                &signers,
                "token_transaction",
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
        self.transaction_handler
            .build_and_send_transaction_with_priority(
                vec![burn_instruction],
                &signers,
                "token_transaction", // Use Settlement priority for burning
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
        self.transaction_handler
            .build_and_send_transaction_with_priority(
                vec![transfer_instruction],
                &signers,
                "token_transaction",
            )
            .await
    }

    /// Transfer SPL tokens from one account to another (generic)
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
        self.transaction_handler
            .build_and_send_transaction_with_priority(
                vec![transfer_instruction],
                &signers,
                "token_transaction",
            )
            .await
    }
}
