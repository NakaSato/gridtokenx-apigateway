use anyhow::{anyhow, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use tracing::{debug, info, warn};

pub struct SigningManager;

impl SigningManager {
    pub async fn sign_transaction(
        transaction: &mut Transaction,
        payer: &Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<Signature> {
        transaction
            .try_sign(&[payer], recent_blockhash)
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

        debug!("Transaction signed successfully");
        Ok(transaction.signatures[0])
    }

    pub async fn get_payer_keypair() -> Result<Keypair> {
        // Try loading from secure storage first
        if let Ok(keypair) = Self::load_payer_keypair().await {
            return Ok(keypair);
        }

        // Fallback to environment variable
        if let Ok(private_key) = std::env::var("PAYER_PRIVATE_KEY") {
            if let Ok(key_bytes) = bs58::decode(&private_key).into_vec() {
                if key_bytes.len() == 64 {
                    let mut secret_key = [0u8; 32];
                    secret_key.copy_from_slice(&key_bytes[..32]);
                    return Ok(Keypair::new_from_array(secret_key));
                } else if key_bytes.len() == 32 {
                    let mut secret_key = [0u8; 32];
                    secret_key.copy_from_slice(&key_bytes);
                    return Ok(Keypair::new_from_array(secret_key));
                }
            }
        }

        warn!("Using fallback keypair - set PAYER_PRIVATE_KEY for production");
        Ok(Keypair::new())
    }

    async fn load_payer_keypair() -> Result<Keypair> {
        let key_paths = vec![
            "/run/secrets/payer.json",
            "/app/payer.json",
            "/etc/gridtokenx/payer.json",
        ];

        for path in key_paths {
            if let Ok(keypair) = crate::services::blockchain::utils::BlockchainUtils::load_keypair_from_file(path) {
                info!("Loaded payer keypair from: {}", path);
                return Ok(keypair);
            }
        }

        Err(anyhow!("Payer keypair not found in secure storage"))
    }
}
