use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

/// Canonical message format for meter reading signatures
/// This ensures both simulator and API gateway create identical messages
#[derive(Debug, Serialize, Deserialize)]
pub struct MeterReadingMessage {
    pub meter_serial: String,
    pub timestamp: String,  // ISO 8601 format
    pub kwh_amount: String, // Fixed precision string
    pub wallet: String,     // Base58 wallet address
}

impl MeterReadingMessage {
    /// Create a new message from reading data
    pub fn new(
        meter_serial: String,
        timestamp: chrono::DateTime<chrono::Utc>,
        kwh_amount: rust_decimal::Decimal,
        wallet: String,
    ) -> Self {
        Self {
            meter_serial,
            timestamp: timestamp.to_rfc3339(),
            kwh_amount: format!("{:.6}", kwh_amount), // 6 decimal places
            wallet,
        }
    }

    /// Convert to canonical string format for signing/verification
    pub fn to_canonical_string(&self) -> String {
        format!(
            "GRIDTOKENX_METER_READING\nmeter_serial: {}\ntimestamp: {}\nkwh_amount: {}\nwallet: {}",
            self.meter_serial, self.timestamp, self.kwh_amount, self.wallet
        )
    }

    /// Get bytes for signing/verification
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_canonical_string().into_bytes()
    }
}

/// Verify Ed25519 signature for a meter reading
pub fn verify_signature(
    public_key_base58: &str,
    signature_base58: &str,
    message: &MeterReadingMessage,
) -> Result<bool, String> {
    debug!("Verifying signature for meter: {}", message.meter_serial);

    // Decode public key from base58
    let public_key_bytes = bs58::decode(public_key_base58)
        .into_vec()
        .map_err(|e| format!("Invalid public key base58: {}", e))?;

    if public_key_bytes.len() != 32 {
        return Err(format!(
            "Invalid public key length: expected 32 bytes, got {}",
            public_key_bytes.len()
        ));
    }

    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "Invalid public key length".to_string())?;

    let public_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|e| format!("Invalid public key: {}", e))?;

    // Decode signature from base58
    let signature_bytes = bs58::decode(signature_base58)
        .into_vec()
        .map_err(|e| format!("Invalid signature base58: {}", e))?;

    if signature_bytes.len() != 64 {
        return Err(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature_bytes.len()
        ));
    }

    let signature_array: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| "Invalid signature length".to_string())?;

    let signature = Signature::from_bytes(&signature_array);

    // Get message bytes
    let message_bytes = message.to_bytes();

    // Verify signature
    match public_key.verify(&message_bytes, &signature) {
        Ok(_) => {
            debug!("Signature verification successful");
            Ok(true)
        }
        Err(e) => {
            error!("Signature verification failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
    use rand::rngs::OsRng;
    use rand::RngCore;

    fn generate_signing_key() -> SigningKey {
        let mut csprng = OsRng;
        let mut bytes = [0u8; 32];
        csprng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    #[test]
    fn test_canonical_message_format() {
        let message = MeterReadingMessage {
            meter_serial: "METER-123".to_string(),
            timestamp: "2025-12-03T04:00:00Z".to_string(),
            kwh_amount: "5.123456".to_string(),
            wallet: "5KQwr...".to_string(),
        };

        let canonical = message.to_canonical_string();
        assert!(canonical.contains("GRIDTOKENX_METER_READING"));
        assert!(canonical.contains("meter_serial: METER-123"));
        assert!(canonical.contains("kwh_amount: 5.123456"));
    }

    #[test]
    fn test_signature_verification() {
        // Generate keypair
        let signing_key = generate_signing_key();

        // Create message
        let message = MeterReadingMessage {
            meter_serial: "METER-123".to_string(),
            timestamp: "2025-12-03T04:00:00Z".to_string(),
            kwh_amount: "5.123456".to_string(),
            wallet: "5KQwr...".to_string(),
        };

        // Sign message
        let signature = signing_key.sign(&message.to_bytes());

        // Encode to base58
        let public_key_base58 = bs58::encode(signing_key.verifying_key().as_bytes()).into_string();
        let signature_base58 = bs58::encode(signature.to_bytes()).into_string();

        // Verify
        let result = verify_signature(&public_key_base58, &signature_base58, &message);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        // Generate two different keypairs
        let signing_key1 = generate_signing_key();
        let signing_key2 = generate_signing_key();

        // Create message
        let message = MeterReadingMessage {
            meter_serial: "METER-123".to_string(),
            timestamp: "2025-12-03T04:00:00Z".to_string(),
            kwh_amount: "5.123456".to_string(),
            wallet: "5KQwr...".to_string(),
        };

        // Sign with signing_key1
        let signature = signing_key1.sign(&message.to_bytes());

        // Try to verify with signing_key2's public key
        let public_key_base58 = bs58::encode(signing_key2.verifying_key().as_bytes()).into_string();
        let signature_base58 = bs58::encode(signature.to_bytes()).into_string();

        // Verify should fail
        let result = verify_signature(&public_key_base58, &signature_base58, &message);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should be false
    }
}
