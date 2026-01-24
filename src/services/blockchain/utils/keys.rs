use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use std::str::FromStr;
use tracing::info;

pub struct KeyUtils;

impl KeyUtils {
    pub fn parse_pubkey(pubkey_str: &str) -> Result<Pubkey> {
        Pubkey::from_str(pubkey_str)
            .map_err(|e| anyhow!("Invalid public key '{}': {}", pubkey_str, e))
    }

    pub fn load_keypair_from_file(filepath: &str) -> Result<Keypair> {
        use std::fs;
        info!("Loading keypair from file: {}", filepath);

        let file_contents = fs::read_to_string(filepath)
            .map_err(|e| anyhow!("Failed to read keypair file '{}': {}", filepath, e))?;

        let bytes: Vec<u8> = serde_json::from_str(&file_contents)
            .map_err(|e| anyhow!("Failed to parse keypair JSON: {}", e))?;

        if bytes.len() != 64 {
            return Err(anyhow!("Invalid keypair file: expected 64 bytes, got {}", bytes.len()));
        }

        let mut secret_key = [0u8; 32];
        secret_key.copy_from_slice(&bytes[0..32]);
        Ok(Keypair::new_from_array(secret_key))
    }
}
