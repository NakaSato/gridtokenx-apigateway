use rand::Rng;
use sha2::{Digest, Sha256};

pub mod types;

/// Service for generating and managing verification tokens
pub struct TokenService;

impl TokenService {
    /// Generate a cryptographically secure random verification token
    ///
    /// Returns a 32-byte random token encoded as a base58 string (similar to Solana addresses)
    /// This provides ~256 bits of entropy, making tokens virtually impossible to guess
    pub fn generate_verification_token() -> String {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        bs58::encode(bytes).into_string()
    }

    /// Hash a token using SHA-256 for secure database storage
    ///
    /// # Arguments
    /// * `token` - The plain text token to hash
    ///
    /// # Returns
    /// Lowercase hexadecimal string representation of the SHA-256 hash
    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Verify that a plain text token matches a stored hash
    ///
    /// # Arguments
    /// * `token` - The plain text token to verify
    /// * `hash` - The stored hash to compare against
    ///
    /// # Returns
    /// `true` if the token matches the hash, `false` otherwise
    pub fn verify_token(token: &str, hash: &str) -> bool {
        Self::hash_token(token) == hash
    }

    /// Generate a short numeric verification code (6 digits)
    ///
    /// Useful for SMS or backup verification codes
    /// Less secure than full tokens but more user-friendly
    pub fn generate_short_code() -> String {
        let mut rng = rand::thread_rng();
        let code: u32 = rng.gen_range(100000..999999);
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_verification_token() {
        let token1 = TokenService::generate_verification_token();
        let token2 = TokenService::generate_verification_token();

        // Tokens should be non-empty
        assert!(!token1.is_empty());
        assert!(!token2.is_empty());

        // Tokens should be different (extremely high probability)
        assert_ne!(token1, token2);

        // Tokens should be base58 encoded (valid characters only)
        assert!(token1
            .chars()
            .all(|c| "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c)));
    }

    #[test]
    fn test_hash_token() {
        let token = "test_token_123";
        let hash = TokenService::hash_token(token);

        // Hash should be 64 characters (256 bits in hex)
        assert_eq!(hash.len(), 64);

        // Hash should be deterministic
        let hash2 = TokenService::hash_token(token);
        assert_eq!(hash, hash2);

        // Different tokens should produce different hashes
        let different_hash = TokenService::hash_token("different_token");
        assert_ne!(hash, different_hash);

        // Hash should be lowercase hex
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn test_verify_token() {
        let token = "my_secret_token_xyz";
        let hash = TokenService::hash_token(token);

        // Correct token should verify
        assert!(TokenService::verify_token(token, &hash));

        // Incorrect token should not verify
        assert!(!TokenService::verify_token("wrong_token", &hash));

        // Empty token should not verify
        assert!(!TokenService::verify_token("", &hash));
    }

    #[test]
    fn test_generate_short_code() {
        let code1 = TokenService::generate_short_code();
        let code2 = TokenService::generate_short_code();

        // Codes should be 6 digits
        assert_eq!(code1.len(), 6);
        assert_eq!(code2.len(), 6);

        // Codes should be numeric
        assert!(code1.chars().all(|c| c.is_ascii_digit()));
        assert!(code2.chars().all(|c| c.is_ascii_digit()));

        // Codes should be in valid range
        let num1: u32 = code1.parse().unwrap();
        let num2: u32 = code2.parse().unwrap();
        assert!(num1 >= 100000 && num1 <= 999999);
        assert!(num2 >= 100000 && num2 <= 999999);
    }

    #[test]
    fn test_token_collision_resistance() {
        // Generate multiple tokens and ensure no collisions
        let mut tokens = std::collections::HashSet::new();
        for _ in 0..1000 {
            let token = TokenService::generate_verification_token();
            assert!(!tokens.contains(&token), "Token collision detected!");
            tokens.insert(token);
        }
    }

    #[test]
    fn test_hash_consistency() {
        // Same token should always produce same hash
        let token = "consistency_test_token";
        let hashes: Vec<String> = (0..10).map(|_| TokenService::hash_token(token)).collect();

        let first_hash = &hashes[0];
        assert!(hashes.iter().all(|h| h == first_hash));
    }
}
