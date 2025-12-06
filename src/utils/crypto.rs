use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2;
use rand::RngCore;
use sha2::Sha256;

// Constants for PBKDF2
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32; // AES-256
const ITERATIONS: u32 = 100_000;
// AES-GCM standard nonce size (12 bytes / 96-bit)
const NONCE_LEN: usize = 12;

/// Encrypt data using AES-GCM with a key derived from the master secret using PBKDF2
/// Returns (encrypted_data_base64, salt_base64, nonce_base64)
pub fn encrypt(data: &[u8], master_secret: &str) -> Result<(String, String, String)> {
    // 1. Generate random salt
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    // 2. Derive key from master secret + salt
    let mut key_bytes = [0u8; KEY_LEN];
    pbkdf2::<hmac::Hmac<Sha256>>(master_secret.as_bytes(), &salt, ITERATIONS, &mut key_bytes)
        .map_err(|e| anyhow!("PBKDF2 error: {}", e))?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 3. Generate random nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message

    // 4. Encrypt
    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| anyhow!("Encryption failure: {}", e))?;

    // 5. Encode everything as base64
    let encrypted_b64 = BASE64.encode(ciphertext);
    let salt_b64 = BASE64.encode(salt);
    let nonce_b64 = BASE64.encode(nonce);

    Ok((encrypted_b64, salt_b64, nonce_b64))
}

/// Encrypt data and return raw bytes (for database storage)
pub fn encrypt_to_bytes(data: &[u8], master_secret: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    // 1. Generate random salt
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    // 2. Derive key from master secret + salt
    let mut key_bytes = [0u8; KEY_LEN];
    pbkdf2::<hmac::Hmac<Sha256>>(master_secret.as_bytes(), &salt, ITERATIONS, &mut key_bytes)
        .map_err(|e| anyhow!("PBKDF2 error: {}", e))?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 3. Generate random nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // 4. Encrypt
    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| anyhow!("Encryption failure: {}", e))?;

    // 5. Return as raw bytes
    Ok((ciphertext, salt.to_vec(), nonce.to_vec()))
}

/// Decrypt data using AES-GCM
/// Supports both standard 12-byte nonces and legacy 16-byte IVs
pub fn decrypt(
    encrypted_data_b64: &str,
    salt_b64: &str,
    nonce_b64: &str,
    master_secret: &str,
) -> Result<Vec<u8>> {
    // 1. Decode base64 inputs
    let ciphertext = BASE64
        .decode(encrypted_data_b64)
        .map_err(|e| anyhow!("Invalid base64 ciphertext: {}", e))?;
    let salt = BASE64
        .decode(salt_b64)
        .map_err(|e| anyhow!("Invalid base64 salt: {}", e))?;
    let nonce_bytes = BASE64
        .decode(nonce_b64)
        .map_err(|e| anyhow!("Invalid base64 nonce: {}", e))?;

    if salt.len() != SALT_LEN {
        return Err(anyhow!("Invalid salt length"));
    }

    // 2. Re-derive key
    let mut key_bytes = [0u8; KEY_LEN];
    pbkdf2::<hmac::Hmac<Sha256>>(master_secret.as_bytes(), &salt, ITERATIONS, &mut key_bytes)
        .map_err(|e| anyhow!("PBKDF2 error: {}", e))?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 3. Handle both 12-byte (standard) and 16-byte (legacy) nonce/IV sizes
    let nonce_to_use: &[u8] = if nonce_bytes.len() == NONCE_LEN {
        // Standard 12-byte nonce
        &nonce_bytes
    } else if nonce_bytes.len() == 16 {
        // Legacy 16-byte IV - truncate to 12 bytes
        // This is a compatibility mode for old encryption format
        tracing::warn!(
            "Found legacy 16-byte IV, truncating to 12 bytes. Consider re-encrypting this data."
        );
        &nonce_bytes[..NONCE_LEN]
    } else {
        return Err(anyhow!(
            "Invalid nonce/IV length: expected {} or 16 bytes, got {}",
            NONCE_LEN,
            nonce_bytes.len()
        ));
    };

    let nonce = Nonce::from_slice(nonce_to_use);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| anyhow!("Decryption failure: {}", e))?;

    Ok(plaintext)
}

/// Decrypt data from raw bytes (convenience wrapper for database storage)
pub fn decrypt_bytes(
    encrypted_data: &[u8],
    salt: &[u8],
    nonce: &[u8],
    master_secret: &str,
) -> Result<Vec<u8>> {
    // Convert bytes to base64 and call main decrypt function
    let encrypted_b64 = BASE64.encode(encrypted_data);
    let salt_b64 = BASE64.encode(salt);
    let nonce_b64 = BASE64.encode(nonce);

    decrypt(&encrypted_b64, &salt_b64, &nonce_b64, master_secret)
}
