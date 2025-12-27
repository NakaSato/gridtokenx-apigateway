use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature};
use std::str::FromStr;
use std::time::Duration; // Added Duration
// Removed tracing as it was unused

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

        let _stdout_str = String::from_utf8_lossy(&output.stdout);
        let _stderr_str = String::from_utf8_lossy(&output.stderr);


        if !output.status.success() {
            if !_stderr_str.contains("already exists") && !_stdout_str.contains("already exists") {
                return Err(anyhow!("spl-token CLI failed: {}", _stderr_str));
            }
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok(ata_address)
    }

    /// Mint energy tokens directly to a user's token account via Anchor program
    /// The mint authority is the token_info PDA, so we must use the Anchor program CPI
    pub async fn mint_energy_tokens(
        &self,
        authority: &Keypair,
        _user_token_account: &Pubkey, // Not used directly - we derive from wallet
        user_wallet: &Pubkey,
        _mint: &Pubkey, // Not used directly - we derive from program
        amount_kwh: f64,
    ) -> Result<Signature> {
        use solana_sdk::instruction::Instruction;
        use solana_sdk::signature::Signer;

        // Convert kWh to token amount (with 9 decimals)
        let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;

        // Get energy token program ID from environment (with fallback to deployed program ID)
        let energy_token_program_id = std::env::var("SOLANA_ENERGY_TOKEN_PROGRAM_ID")
            .unwrap_or_else(|_| "HaT3koMseafcCB9aUQUCrSLMDfN1km7Xik9UhZSG9UV6".to_string());
        let energy_token_program_id = Pubkey::from_str(&energy_token_program_id)
            .map_err(|e| anyhow!("Invalid SOLANA_ENERGY_TOKEN_PROGRAM_ID: {}", e))?;

        // Derive the mint PDA from energy_token program
        let (mint_pda, _) = Pubkey::find_program_address(
            &[b"mint"],
            &energy_token_program_id,
        );

        // Derive token_info PDA (this is the mint authority)
        let (token_info_pda, _) = Pubkey::find_program_address(
            &[b"token_info"],
            &energy_token_program_id,
        );

        // Get the token program ID
        let token_program_id = BlockchainUtils::get_token_program_id()?;

        // Calculate ATA for the user
        let user_token_account = spl_associated_token_account::get_associated_token_address_with_program_id(
            user_wallet,
            &mint_pda,
            &token_program_id,
        );

        // Build instructions
        let mut instructions = Vec::new();

        // 1. Create ATA if it doesn't exist (idempotent)
        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &authority.pubkey(),
            user_wallet,
            &mint_pda,
            &token_program_id,
        );
        instructions.push(create_ata_ix);

        // 2. Build the Anchor mint_tokens_direct instruction
        // Discriminator for "mint_tokens_direct": calculated from sha256("global:mint_tokens_direct")[:8]
        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&[13, 246, 31, 237, 99, 19, 88, 226]);
        instruction_data.extend_from_slice(&amount_lamports.to_le_bytes());

        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(token_info_pda, false),
            solana_sdk::instruction::AccountMeta::new(mint_pda, false),
            solana_sdk::instruction::AccountMeta::new(user_token_account, false),
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(token_program_id, false),
        ];

        let mint_instruction = Instruction {
            program_id: energy_token_program_id,
            accounts,
            data: instruction_data,
        };
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

    /// Mint or burn SPL tokens directly using standard spl-token CLI
    /// This is for testing purposes when using a standard SPL token mint
    /// (not the Anchor-based energy token program)
    /// - Positive amounts: mint tokens to the user
    /// - Negative amounts: burn tokens from the user
    pub async fn mint_spl_tokens(
        &self,
        _authority: &Keypair,
        user_wallet: &Pubkey,
        mint: &Pubkey,
        amount_kwh: f64,
    ) -> Result<Signature> {


        let wallet_path = std::env::var("AUTHORITY_WALLET_PATH")
            .unwrap_or_else(|_| "dev-wallet.json".to_string());
        let rpc_url =
            std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://localhost:8899".to_string());

        // Determine if we're minting or burning based on the sign
        let is_burn = amount_kwh < 0.0;
        let amount_abs = amount_kwh.abs();

        if is_burn {
            // BURN tokens from user's account
            // BURN tokens from user's account


            // First, get the user's associated token account
            let get_account_output = std::process::Command::new("spl-token")
                .arg("address")
                .arg("--verbose") // Add --verbose because it's required in this environment
                .arg("--token")
                .arg(mint.to_string())
                .arg("--owner")
                .arg(user_wallet.to_string())
                .arg("--program-id")
                .arg("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb") // Token-2022 Program ID
                .arg("--url")
                .arg(&rpc_url)
                .output()
                .map_err(|e| anyhow!("Failed to get token account: {}", e))?;

            let stdout_str = String::from_utf8_lossy(&get_account_output.stdout);


            if !get_account_output.status.success() {
                let stderr_str = String::from_utf8_lossy(&get_account_output.stderr);
                return Err(anyhow!("Failed to get user's token account: {}", stderr_str));
            }

            // Parse associated token address from verbose output
            let token_account_str = stdout_str
                .lines()
                .find(|line| line.contains("Associated token address:"))
                .and_then(|line| line.split(':').last())
                .map(|s| s.trim().to_string())
                .ok_or_else(|| anyhow!("Failed to parse associated token address from output: {}", stdout_str))?;



            // Burn tokens from the user's account
            let output = std::process::Command::new("spl-token")
                .arg("burn")
                .arg(token_account_str)
                .arg(amount_abs.to_string())
                .arg("--fee-payer")
                .arg(&wallet_path)
                .arg("--owner")
                .arg(&wallet_path)
                .arg("--program-2022")
                .arg("--url")
                .arg(&rpc_url)
                .output()
                .map_err(|e| anyhow!("Failed to execute spl-token burn: {}", e))?;

            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let _stderr_str = String::from_utf8_lossy(&output.stderr);


            if !output.status.success() {
                return Err(anyhow!("spl-token burn failed: {}", _stderr_str));
            }

            // Extract signature from output
            let signature_str = stdout_str
                .lines()
                .find(|line| line.contains("Signature:"))
                .and_then(|line| line.split_whitespace().last())
                .ok_or_else(|| anyhow!("Failed to parse signature from burn output: {}", stdout_str))?;

            let signature = Signature::from_str(signature_str)
                .map_err(|e| anyhow!("Failed to parse signature '{}': {}", signature_str, e))?;

            // Wait for confirmation
            tokio::time::sleep(Duration::from_secs(2)).await;

            Ok(signature)
        } else {
            // MINT tokens to user's account
            // MINT tokens to user's account


            let output = std::process::Command::new("spl-token")
                .arg("mint")
                .arg(mint.to_string())
                .arg(amount_abs.to_string())
                .arg("--recipient-owner")
                .arg(user_wallet.to_string())
                .arg("--fee-payer")
                .arg(&wallet_path)
                .arg("--mint-authority")
                .arg(&wallet_path)
                .arg("--url")
                .arg(&rpc_url)
                .output()
                .map_err(|e| anyhow!("Failed to execute spl-token mint: {}", e))?;

            let _stdout_str = String::from_utf8_lossy(&output.stdout);
            let _stderr_str = String::from_utf8_lossy(&output.stderr);


            if !output.status.success() {
                return Err(anyhow!("spl-token mint failed: {}", _stderr_str));
            }

            // Extract signature from output (format: "Minting X tokens\n  Token: ...\n\nSignature: <sig>")
            let signature_str = _stdout_str
                .lines()
                .find(|line| line.contains("Signature:"))
                .and_then(|line| line.split_whitespace().last())
                .ok_or_else(|| anyhow!("Failed to parse signature from mint output: {}", _stdout_str))?;

            let signature = Signature::from_str(signature_str)
                .map_err(|e| anyhow!("Failed to parse signature '{}': {}", signature_str, e))?;

            // Wait for confirmation
            tokio::time::sleep(Duration::from_secs(2)).await;

            Ok(signature)
        }
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
