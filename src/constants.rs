//! Application constants and configuration values.
//!
//! This module centralizes all hardcoded values and magic numbers
//! to improve maintainability and make it easy to adjust settings.

/// API Version constants
pub mod api {
    /// Current API version
    pub const VERSION: &str = "v1";
    
    /// API version header name
    pub const VERSION_HEADER: &str = "X-API-Version";
    
    /// Deprecated API version header
    pub const DEPRECATED_HEADER: &str = "X-API-Deprecated";
}

/// Authentication and security constants
pub mod auth {
    /// Default JWT expiration in hours
    pub const JWT_EXPIRATION_HOURS: i64 = 24;
    
    /// Refresh token expiration in days
    pub const REFRESH_TOKEN_EXPIRATION_DAYS: i64 = 7;
    
    /// Maximum failed login attempts before lockout
    pub const MAX_LOGIN_ATTEMPTS: u32 = 5;
    
    /// Lockout duration in minutes
    pub const LOCKOUT_DURATION_MINUTES: u32 = 15;
    
    /// Minimum password length
    pub const MIN_PASSWORD_LENGTH: usize = 8;
    
    /// Maximum password length
    pub const MAX_PASSWORD_LENGTH: usize = 128;
    
    /// Password hash cost factor
    pub const BCRYPT_COST: u32 = 12;
}

/// Rate limiting constants
pub mod rate_limit {
    /// Default requests per second
    pub const DEFAULT_RPS: u32 = 100;
    
    /// Burst capacity
    pub const BURST_CAPACITY: u32 = 200;
    
    /// Window size in seconds for sliding window rate limiting
    pub const WINDOW_SIZE_SECONDS: u64 = 60;
    
    /// Maximum requests per IP per minute (unauthenticated)
    pub const MAX_REQUESTS_PER_IP: u32 = 60;
    
    /// Maximum requests per user per minute (authenticated)
    pub const MAX_REQUESTS_PER_USER: u32 = 120;
}

/// Database constants
pub mod database {
    /// Default connection pool size
    pub const DEFAULT_POOL_SIZE: u32 = 100;
    
    /// Minimum connections to maintain
    pub const MIN_CONNECTIONS: u32 = 10;
    
    /// Connection acquire timeout in seconds
    pub const ACQUIRE_TIMEOUT_SECS: u64 = 3;
    
    /// Idle connection timeout in seconds
    pub const IDLE_TIMEOUT_SECS: u64 = 180;
    
    /// Maximum connection lifetime in seconds
    pub const MAX_LIFETIME_SECS: u64 = 900;
    
    /// Statement timeout in seconds
    pub const STATEMENT_TIMEOUT_SECS: u64 = 15;
}

/// Cache constants
pub mod cache {
    /// Default cache TTL in seconds
    pub const DEFAULT_TTL_SECS: u64 = 300;
    
    /// Short TTL for frequently updated data
    pub const SHORT_TTL_SECS: u64 = 60;
    
    /// Long TTL for stable data
    pub const LONG_TTL_SECS: u64 = 3600;
    
    /// Cache key prefix for user data
    pub const USER_PREFIX: &str = "user:";
    
    /// Cache key prefix for session data
    pub const SESSION_PREFIX: &str = "session:";
    
    /// Cache key prefix for rate limiting
    pub const RATE_LIMIT_PREFIX: &str = "rate:";
}

/// Pagination constants
pub mod pagination {
    /// Default page number
    pub const DEFAULT_PAGE: u32 = 1;
    
    /// Default items per page
    pub const DEFAULT_PER_PAGE: u32 = 20;
    
    /// Minimum items per page
    pub const MIN_PER_PAGE: u32 = 1;
    
    /// Maximum items per page
    pub const MAX_PER_PAGE: u32 = 100;
}

/// Energy trading constants
pub mod energy {
    /// Minimum energy amount in kWh
    pub const MIN_ENERGY_KWH: f64 = 0.001;
    
    /// Maximum energy amount in a single transaction
    pub const MAX_ENERGY_KWH: f64 = 10_000_000.0;
    
    /// Minimum price per kWh in tokens
    pub const MIN_PRICE_PER_KWH: f64 = 0.0001;
    
    /// Maximum price per kWh in tokens
    pub const MAX_PRICE_PER_KWH: f64 = 1_000_000.0;
    
    /// Energy token decimals
    pub const TOKEN_DECIMALS: u8 = 9;
    
    /// Standard epoch duration in minutes
    pub const EPOCH_DURATION_MINUTES: u32 = 15;
}

/// Blockchain constants
pub mod blockchain {
    /// Maximum retries for blockchain transactions
    pub const MAX_TRANSACTION_RETRIES: u32 = 3;
    
    /// Transaction confirmation timeout in seconds
    pub const CONFIRMATION_TIMEOUT_SECS: u64 = 60;
    
    /// Minimum priority fee in lamports
    pub const MIN_PRIORITY_FEE_LAMPORTS: u64 = 1000;
    
    /// Maximum priority fee in lamports
    pub const MAX_PRIORITY_FEE_LAMPORTS: u64 = 1_000_000;
    
    /// Compute unit limit for standard transactions
    pub const DEFAULT_COMPUTE_UNITS: u32 = 200_000;
}

/// Meter constants
pub mod meter {
    /// Maximum serial number length
    pub const MAX_SERIAL_LENGTH: usize = 50;
    
    /// Minimum reading interval in seconds
    pub const MIN_READING_INTERVAL_SECS: u64 = 60;
    
    /// Maximum reading value (sanity check)
    pub const MAX_READING_VALUE: f64 = 1_000_000_000.0;
    
    /// Reading precision (decimal places)
    pub const READING_PRECISION: u32 = 6;
}

/// HTTP constants
pub mod http {
    /// Default request timeout in seconds
    pub const REQUEST_TIMEOUT_SECS: u64 = 30;
    
    /// Maximum request body size in bytes (10MB)
    pub const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;
    
    /// Keep-alive timeout in seconds
    pub const KEEP_ALIVE_SECS: u64 = 75;
    
    /// Graceful shutdown timeout in seconds
    pub const SHUTDOWN_TIMEOUT_SECS: u64 = 30;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_validity() {
        // Ensure pagination defaults are within bounds
        assert!(pagination::DEFAULT_PER_PAGE >= pagination::MIN_PER_PAGE);
        assert!(pagination::DEFAULT_PER_PAGE <= pagination::MAX_PER_PAGE);
        
        // Ensure energy bounds are valid
        assert!(energy::MIN_ENERGY_KWH < energy::MAX_ENERGY_KWH);
        assert!(energy::MIN_PRICE_PER_KWH < energy::MAX_PRICE_PER_KWH);
        
        // Ensure auth settings are reasonable
        assert!(auth::MIN_PASSWORD_LENGTH > 0);
        assert!(auth::MIN_PASSWORD_LENGTH < auth::MAX_PASSWORD_LENGTH);
        
        // Ensure blockchain settings are valid
        assert!(blockchain::MIN_PRIORITY_FEE_LAMPORTS < blockchain::MAX_PRIORITY_FEE_LAMPORTS);
    }
}
