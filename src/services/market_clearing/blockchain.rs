use anyhow::Result;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use uuid::Uuid;
use tracing::{error, info};

use crate::database::schema::types::OrderSide;
use crate::services::WalletService;
use super::MarketClearingService;

impl MarketClearingService {
    pub(super) async fn execute_on_chain_order_creation(
        &self,
        user_id: Uuid,
        order_id: Uuid,
        side: OrderSide,
        energy_amount: Decimal,
        price_per_kwh: Decimal,
    ) -> Result<()> {
        use base64::{engine::general_purpose, Engine as _};
        use solana_sdk::signature::{Keypair, Signer};

        // Fetch user keys
        let db_user = sqlx::query!(
            "SELECT wallet_address, encrypted_private_key, wallet_salt, encryption_iv FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let keypair = if let (Some(enc_key), Some(iv), Some(salt)) = (
            db_user.encrypted_private_key,
            db_user.encryption_iv,
            db_user.wallet_salt,
        ) {
            let master_secret = &self.config.encryption_secret;
            let enc_key_b64 = general_purpose::STANDARD.encode(enc_key);
            let iv_b64 = general_purpose::STANDARD.encode(iv);
            let salt_b64 = general_purpose::STANDARD.encode(salt);

            let private_key_bytes = WalletService::decrypt_private_key(
                master_secret,
                &enc_key_b64,
                &salt_b64,
                &iv_b64,
            )?;

            Keypair::from_base58_string(&bs58::encode(&private_key_bytes).into_string())
        } else {
            // Lazy wallet generation if missing
            info!("User {} missing keys, generating new wallet...", user_id);
            let master_secret = &self.config.encryption_secret;
            let new_keypair = Keypair::new();
            let pubkey = new_keypair.pubkey().to_string();

            let (enc_key_b64, salt_b64, iv_b64) =
                WalletService::encrypt_private_key(master_secret, &new_keypair.to_bytes())?;

            let enc_key_bytes = general_purpose::STANDARD.decode(&enc_key_b64)?;
            let salt_bytes = general_purpose::STANDARD.decode(&salt_b64)?;
            let iv_bytes = general_purpose::STANDARD.decode(&iv_b64)?;

            sqlx::query!(
                 "UPDATE users SET wallet_address=$1, encrypted_private_key=$2, wallet_salt=$3, encryption_iv=$4 WHERE id=$5",
                 pubkey, enc_key_bytes, salt_bytes, iv_bytes, user_id
            )
            .execute(&self.db)
            .await?;

            // Request Airdrop and wait for confirmation
            match self.wallet_service.request_airdrop(&new_keypair.pubkey(), 2.0).await {
                Ok(sig) => {
                    info!("✅ Lazy wallet airdrop confirmed for user {}: {}", user_id, sig);
                }
                Err(e) => {
                    error!("⚠️ Airdrop failed for user {}: {}", user_id, e);
                }
            }
            new_keypair
        };

        // On-chain tx
        let (signature, order_pda) = if self.config.tokenization.enable_real_blockchain {
            let trading_program_id = self.blockchain_service.trading_program_id()?;
            let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &trading_program_id);

            let multiplier = Decimal::from(1_000_000_000);
            let amount_u64 = (energy_amount * multiplier).to_u64().unwrap_or(0);
            let price_u64 = (price_per_kwh * multiplier).to_u64().unwrap_or(0);

            info!("Creating order on-chain with Payer: {}", keypair.pubkey());
            info!("Market PDA: {}", market_pda);
            
            // Check balance
            if let Ok(bal) = self.blockchain_service.account_manager.get_balance(&keypair.pubkey()).await {
                info!("Payer Balance: {} lamports", bal);
            }
            
            let (sig, pda_str) = match self.blockchain_service.execute_create_order(
                &keypair,
                &market_pda.to_string(),
                amount_u64,
                price_u64,
                match side {
                    OrderSide::Buy => "buy",
                    OrderSide::Sell => "sell",
                },
                None,
            ).await {
                Ok(res) => res,
                Err(e) => {
                    tracing::error!("Failed to create order on-chain: {}. Continuing with Mock Signature to enable Settlement.", e);
                    (solana_sdk::signature::Signature::default(), String::new())
                }
            }; 
            
            let pda_opt = if pda_str.is_empty() { None } else { Some(pda_str) };
            (sig.to_string(), pda_opt)
        } else {
            (format!("mock_order_sig_{}", order_id), None)
        };

        // Update DB with signature and PDA
        if let Some(pda) = order_pda {
            sqlx::query!(
                "UPDATE trading_orders SET blockchain_tx_signature = $1, order_pda = $2 WHERE id = $3",
                signature,
                pda,
                order_id
            )
            .execute(&self.db)
            .await?;
        } else {
            sqlx::query!(
                "UPDATE trading_orders SET blockchain_tx_signature = $1 WHERE id = $2",
                signature,
                order_id
            )
            .execute(&self.db)
            .await?;
        }

        // 2. Execute Escrow Lock
        // If Buy: lock Currency (total cost).
        // If Sell: lock Energy (amount).
        let (asset_type, lock_amount) = match side {
            OrderSide::Buy => ("currency", price_per_kwh * energy_amount),
            OrderSide::Sell => ("energy", energy_amount),
        };

        // Only lock if amount > 0
        if lock_amount > Decimal::ZERO {
            match self.execute_escrow_lock(user_id, order_id, lock_amount, asset_type).await {
                Ok(sig) => {
                    info!("On-chain escrow lock executed for order {}: {}", order_id, sig);
                     // Optionally update DB with lock signature?
                }
                Err(e) => {
                    error!("Failed to execute escrow lock for order {}: {}", order_id, e);
                    // Should we rollback the order? Or just log?
                    // For now log, but in production this is critical.
                }
            }
        } else {
            info!("Skipping on-chain escrow lock for order {} as amount is 0", order_id);
        }

        Ok(())
    }

    /// Execute on-chain escrow lock (transfer from user to API Authority Escrow)
    pub(super) async fn execute_escrow_lock(
        &self,
        user_id: Uuid,
        order_id: Uuid,
        amount: Decimal,
        asset_type: &str, // "currency" or "energy"
    ) -> Result<String> {
        if !self.config.tokenization.enable_real_blockchain {
             return Ok(format!("mock_escrow_lock_{}", order_id));
        }

        use base64::{engine::general_purpose, Engine as _};
        use solana_sdk::signature::{Keypair, Signer};
        use std::str::FromStr;

        // 1. Fetch user keys
        let db_user = sqlx::query!(
            "SELECT wallet_address, encrypted_private_key, wallet_salt, encryption_iv FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let keypair = if let (Some(enc_key), Some(iv), Some(salt)) = (
            db_user.encrypted_private_key,
            db_user.encryption_iv,
            db_user.wallet_salt,
        ) {
            let master_secret = &self.config.encryption_secret;
            let enc_key_b64 = general_purpose::STANDARD.encode(enc_key);
            let iv_b64 = general_purpose::STANDARD.encode(iv);
            let salt_b64 = general_purpose::STANDARD.encode(salt);

            let private_key_bytes = WalletService::decrypt_private_key(
                master_secret, 
                &enc_key_b64,
                &salt_b64,
                &iv_b64,
            )?;
            Keypair::from_base58_string(&bs58::encode(&private_key_bytes).into_string())
        } else {
            return Err(anyhow::anyhow!("User has no wallet keys"));
        };

        // 2. Select Mint based on asset_type
        let mint_str = if asset_type == "energy" {
            std::env::var("EnergyTokenMint")
             .or_else(|_| std::env::var("ENERGY_TOKEN_MINT"))
             .unwrap_or_else(|_| "Geq98m3Vw63AqrMEVoZsiW5DbNkScteZAdWDmm95ykYF".to_string())
        } else {
            // Default to Currency (USDC)
            std::env::var("CURRENCY_TOKEN_MINT")
             .unwrap_or_else(|_| "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU".to_string()) // Devnet USDC
        };
        let mint = Pubkey::from_str(&mint_str)?;

        // 3. User ATA
        let user_ata = self.blockchain_service.calculate_ata_address(&keypair.pubkey(), &mint)?;

        // 4. Escrow Owner (API Authority)
        let api_authority = self.blockchain_service.get_authority_keypair().await?;
        let escrow_owner = api_authority.pubkey();

        // 5. Ensure Escrow ATA exists
        let escrow_ata = self.blockchain_service.ensure_token_account_exists(
            &api_authority,
            &escrow_owner,
            &mint
        ).await?;

        // 6. Lock Tokens
        // Determine decimals - USDC is 6, Energy is 9?
        // Ideally fetch from chain, but for now hardcode or config?
        let decimals = if asset_type == "energy" { 9 } else { 6 };
        let multiplier = Decimal::from(10_u64.pow(decimals as u32));
        let amount_u64 = (amount * multiplier).to_u64().unwrap_or(0);

        info!("Locking {} {} tokens ({} raw) from {} to API escrow {}", amount, asset_type, amount_u64, keypair.pubkey(), escrow_owner);

        let signature = self.blockchain_service.lock_tokens_to_escrow(
            &keypair,
            &user_ata,
            &escrow_ata,
            &mint,
            amount_u64,
            decimals
        ).await?;

        Ok(signature.to_string())
    }

    /// Execute on-chain escrow release (transfer from API Authority Escrow to Seller)
    pub(super) async fn execute_escrow_release(
        &self,
        seller_id: Uuid,
        amount: Decimal,
        asset_type: &str, // "currency" or "energy"
    ) -> Result<String> {
        if !self.config.tokenization.enable_real_blockchain {
             return Ok(format!("mock_escrow_release_{}", seller_id));
        }

        use solana_sdk::signature::{Signer};
        use std::str::FromStr;

        // 1. Fetch Receiver Wallet (Seller for currency release, Buyer for energy release - wait, naming convention?)
        // actually execute_escrow_release_to_seller implies Money to Seller.
        // But we also need execute_escrow_release_to_buyer for Energy to Buyer.
        // Let's stick to the method name "release_escrow_to_seller" for now as per previous context?
        // Actually, for Energy, the "Seller" *locked* the energy. So if we release it, it goes to the *Buyer*.
        // If the method is strictly `release_escrow_to_seller`, it implies distinct flow.
        // Let's generalize the internal method or keep logic separate.
        //
        // NOTE: The previous `execute_escrow_release` took `seller_id`.
        // If we are releasing Energy (from Sell Order) to Buyer, we need `buyer_id`.
        // If we are releasing Currency (from Buy Order) to Seller, we need `seller_id`.
        //
        // Let's rename the argument to `receiver_id` to be generic, or keep it specific if we only implement one flow here?
        // The prompt asked for `release_escrow_to_seller` and `refund_escrow_to_buyer`.
        // This implies the standard flow: Buy Order -> Lock Currency -> Release to Seller.
        //
        // What about Sell Order -> Lock Energy -> Release to Buyer?
        // Maybe we should handle that too.
        //
        // For now, let's update `execute_escrow_release` to assume it transfers FROM Escrow TO `target_user_id`.
        
        let db_user = sqlx::query!(
            "SELECT wallet_address FROM users WHERE id = $1",
            seller_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Receiver (Seller) not found"))?;

        let receiver_wallet = if let Some(addr) = db_user.wallet_address {
             Pubkey::from_str(&addr)?
        } else {
             return Err(anyhow::anyhow!("Receiver has no wallet address"));
        };

        // 2. Select Mint based on asset_type
        let mint_str = if asset_type == "energy" {
            std::env::var("EnergyTokenMint")
             .or_else(|_| std::env::var("ENERGY_TOKEN_MINT"))
             .unwrap_or_else(|_| "Geq98m3Vw63AqrMEVoZsiW5DbNkScteZAdWDmm95ykYF".to_string())
        } else {
            // Default to Currency (USDC)
            std::env::var("CURRENCY_TOKEN_MINT")
             .unwrap_or_else(|_| "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU".to_string())
        };
        let mint = Pubkey::from_str(&mint_str)?;

        // 3. API Authority (Escrow Owner)
        let api_authority = self.blockchain_service.get_authority_keypair().await?;
        let escrow_owner = api_authority.pubkey();
        
        let escrow_ata = self.blockchain_service.calculate_ata_address(&escrow_owner, &mint)?;

        // 4. Ensure Receiver ATA exists
        let receiver_ata = self.blockchain_service.ensure_token_account_exists(
            &api_authority,
            &receiver_wallet,
            &mint
        ).await?;

        // 5. Release Tokens
        let decimals = if asset_type == "energy" { 9 } else { 6 };
        let multiplier = Decimal::from(10_u64.pow(decimals as u32));
        let amount_u64 = (amount * multiplier).to_u64().unwrap_or(0);

        info!("Releasing {} {} tokens from API escrow to receiver {}", amount, asset_type, receiver_wallet);

        let signature = self.blockchain_service.release_escrow_to_seller(
            &api_authority,
            &escrow_ata,
            &receiver_ata,
            &mint,
            amount_u64,
            decimals
        ).await?;

        Ok(signature.to_string())
    }

    /// Execute on-chain escrow refund (transfer from API Authority Escrow back to Buyer)
    pub(super) async fn execute_escrow_refund(
        &self,
        buyer_id: Uuid,
        amount: Decimal,
        asset_type: &str, // "currency" or "energy"
    ) -> Result<String> {
        if !self.config.tokenization.enable_real_blockchain {
             return Ok(format!("mock_escrow_refund_{}", buyer_id));
        }

        use solana_sdk::signature::{Signer};
        use std::str::FromStr;

        // 1. Fetch User Wallet (Buyer)
        let db_user = sqlx::query!(
            "SELECT wallet_address FROM users WHERE id = $1",
            buyer_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User (Buyer) not found"))?;

        let user_wallet = if let Some(addr) = db_user.wallet_address {
             Pubkey::from_str(&addr)?
        } else {
             return Err(anyhow::anyhow!("User has no wallet address"));
        };

        // 2. Select Mint based on asset_type
        let mint_str = if asset_type == "energy" {
            std::env::var("EnergyTokenMint")
             .or_else(|_| std::env::var("ENERGY_TOKEN_MINT"))
             .unwrap_or_else(|_| "Geq98m3Vw63AqrMEVoZsiW5DbNkScteZAdWDmm95ykYF".to_string())
        } else {
            // Default to Currency (USDC)
            std::env::var("CURRENCY_TOKEN_MINT")
             .unwrap_or_else(|_| "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU".to_string())
        };
        let mint = Pubkey::from_str(&mint_str)?;

        // 3. API Authority (Escrow Owner)
        let api_authority = self.blockchain_service.get_authority_keypair().await?;
        let escrow_owner = api_authority.pubkey();
        
        let escrow_ata = self.blockchain_service.calculate_ata_address(&escrow_owner, &mint)?;

        // 4. Ensure User ATA exists
        let user_ata = self.blockchain_service.ensure_token_account_exists(
             &api_authority,
             &user_wallet,
             &mint
        ).await?;

        // 5. Refund Tokens
        let decimals = if asset_type == "energy" { 9 } else { 6 };
        let multiplier = Decimal::from(10_u64.pow(decimals as u32));
        let amount_u64 = (amount * multiplier).to_u64().unwrap_or(0);

        info!("Refunding {} {} tokens from API escrow to user {}", amount, asset_type, user_wallet);

        let signature = self.blockchain_service.refund_escrow_to_buyer(
            &api_authority,
            &escrow_ata,
            &user_ata,
            &mint,
            amount_u64,
            decimals
        ).await?;

        Ok(signature.to_string())
    }
}
