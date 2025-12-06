use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

pub mod tokenization;
pub use tokenization::{TokenizationConfig, ValidationError};
// Removed unused imports: ConfigError

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub environment: String,
    pub port: u16,
    pub database_url: String,
    pub influxdb_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_expiration: i64,
    pub solana_rpc_url: String,
    pub solana_ws_url: String,
    pub energy_token_mint: String,
    pub engineering_api_key: String,
    pub max_connections: u32,
    pub redis_pool_size: u32,
    pub request_timeout: u64,
    pub rate_limit_window: u64,
    pub log_level: String,
    pub audit_log_enabled: bool,
    pub test_mode: bool,
    pub email: EmailConfig,
    pub tokenization: TokenizationConfig,
    pub event_processor: EventProcessorConfig,
    pub solana_programs: SolanaProgramsConfig,
    /// Default simulator user UUID for engineering/test mode
    pub simulator_user_id: String,
    pub encryption_secret: String,
}

/// Solana program IDs configuration - moved from hardcoded values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaProgramsConfig {
    pub registry_program_id: String,
    pub oracle_program_id: String,
    pub governance_program_id: String,
    pub energy_token_program_id: String,
    pub trading_program_id: String,
}

impl Default for SolanaProgramsConfig {
    fn default() -> Self {
        Self {
            registry_program_id: "GTX1111111111111111111111111111111111111111".to_string(),
            oracle_program_id: "GTX2222222222222222222222222222222222222222".to_string(),
            governance_program_id: "GTX3333333333333333333333333333333333333333".to_string(),
            energy_token_program_id: "GTX4444444444444444444444444444444444444444".to_string(),
            trading_program_id: "GTX5555555555555555555555555555555555555555".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventProcessorConfig {
    pub enabled: bool,
    pub polling_interval_secs: u64,
    pub batch_size: usize,
    pub max_retries: u32,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_name: String,
    pub from_address: String,
    pub verification_expiry_hours: i64,
    pub verification_base_url: String,
    pub verification_required: bool,
    pub verification_enabled: bool,
    pub auto_login_after_verification: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env file if it exists

        Ok(Config {
            environment: env::var("ENVIRONMENT")
                .map_err(|_| anyhow::anyhow!("ENVIRONMENT environment variable is required"))?,
            port: env::var("PORT")
                .map_err(|_| anyhow::anyhow!("PORT environment variable is required"))?
                .parse()?,
            database_url: env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?,
            influxdb_url: env::var("INFLUXDB_URL")
                .unwrap_or_else(|_| "http://localhost:8086".to_string()),
            redis_url: env::var("REDIS_URL")
                .map_err(|_| anyhow::anyhow!("REDIS_URL environment variable is required"))?,
            jwt_secret: env::var("JWT_SECRET")
                .map_err(|_| anyhow::anyhow!("JWT_SECRET environment variable is required"))?,
            jwt_expiration: env::var("JWT_EXPIRATION")
                .unwrap_or_else(|_| "86400".to_string())
                .parse()
                .unwrap_or(86400),
            solana_rpc_url: env::var("SOLANA_RPC_URL")
                .map_err(|_| anyhow::anyhow!("SOLANA_RPC_URL environment variable is required"))?,
            solana_ws_url: env::var("SOLANA_WS_URL")
                .map_err(|_| anyhow::anyhow!("SOLANA_WS_URL environment variable is required"))?,
            energy_token_mint: env::var("ENERGY_TOKEN_MINT").map_err(|_| {
                anyhow::anyhow!("ENERGY_TOKEN_MINT environment variable is required")
            })?,
            engineering_api_key: env::var("ENGINEERING_API_KEY").map_err(|_| {
                anyhow::anyhow!("ENGINEERING_API_KEY environment variable is required")
            })?,
            max_connections: env::var("MAX_CONNECTIONS")
                .map_err(|_| anyhow::anyhow!("MAX_CONNECTIONS environment variable is required"))?
                .parse()?,
            redis_pool_size: env::var("REDIS_POOL_SIZE")
                .map_err(|_| anyhow::anyhow!("REDIS_POOL_SIZE environment variable is required"))?
                .parse()?,
            request_timeout: env::var("REQUEST_TIMEOUT")
                .map_err(|_| anyhow::anyhow!("REQUEST_TIMEOUT environment variable is required"))?
                .parse()?,
            rate_limit_window: env::var("RATE_LIMIT_WINDOW")
                .map_err(|_| anyhow::anyhow!("RATE_LIMIT_WINDOW environment variable is required"))?
                .parse()?,
            log_level: env::var("LOG_LEVEL")
                .map_err(|_| anyhow::anyhow!("LOG_LEVEL environment variable is required"))?,
            audit_log_enabled: env::var("AUDIT_LOG_ENABLED")
                .map_err(|_| anyhow::anyhow!("AUDIT_LOG_ENABLED environment variable is required"))?
                .parse()?,
            test_mode: env::var("TEST_MODE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            email: EmailConfig {
                smtp_host: env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string()),
                smtp_port: env::var("SMTP_PORT")
                    .unwrap_or_else(|_| "587".to_string())
                    .parse()
                    .unwrap_or(587),
                smtp_username: env::var("SMTP_USERNAME")
                    .unwrap_or_else(|_| "noreply@gridtokenx.com".to_string()),
                smtp_password: env::var("SMTP_PASSWORD").unwrap_or_else(|_| "".to_string()),
                from_name: env::var("EMAIL_FROM_NAME")
                    .unwrap_or_else(|_| "GridTokenX Platform".to_string()),
                from_address: env::var("EMAIL_FROM_ADDRESS")
                    .unwrap_or_else(|_| "noreply@gridtokenx.com".to_string()),
                verification_expiry_hours: env::var("EMAIL_VERIFICATION_EXPIRY_HOURS")
                    .unwrap_or_else(|_| "24".to_string())
                    .parse()
                    .unwrap_or(24),
                verification_base_url: env::var("EMAIL_VERIFICATION_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                verification_required: env::var("EMAIL_VERIFICATION_REQUIRED")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
                verification_enabled: env::var("EMAIL_VERIFICATION_ENABLED")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
                auto_login_after_verification: env::var("EMAIL_AUTO_LOGIN_AFTER_VERIFICATION")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
            },
            tokenization: TokenizationConfig::from_env()
                .map_err(|e| anyhow::anyhow!("Failed to load tokenization config: {}", e))?,
            event_processor: EventProcessorConfig {
                enabled: env::var("EVENT_PROCESSOR_ENABLED")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
                polling_interval_secs: env::var("EVENT_PROCESSOR_POLLING_INTERVAL_SECS")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()
                    .unwrap_or(10),
                batch_size: env::var("EVENT_PROCESSOR_BATCH_SIZE")
                    .unwrap_or_else(|_| "100".to_string())
                    .parse()
                    .unwrap_or(100),
                max_retries: env::var("EVENT_PROCESSOR_MAX_RETRIES")
                    .unwrap_or_else(|_| "3".to_string())
                    .parse()
                    .unwrap_or(3),
                webhook_url: env::var("EVENT_PROCESSOR_WEBHOOK_URL").ok(),
                webhook_secret: env::var("EVENT_PROCESSOR_WEBHOOK_SECRET").ok(),
            },
            solana_programs: SolanaProgramsConfig {
                registry_program_id: env::var("SOLANA_REGISTRY_PROGRAM_ID")
                    .unwrap_or_else(|_| "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7".to_string()),
                oracle_program_id: env::var("SOLANA_ORACLE_PROGRAM_ID")
                    .unwrap_or_else(|_| "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE".to_string()),
                governance_program_id: env::var("SOLANA_GOVERNANCE_PROGRAM_ID")
                    .unwrap_or_else(|_| "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe".to_string()),
                energy_token_program_id: env::var("SOLANA_ENERGY_TOKEN_PROGRAM_ID")
                    .unwrap_or_else(|_| "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur".to_string()),
                trading_program_id: env::var("SOLANA_TRADING_PROGRAM_ID")
                    .unwrap_or_else(|_| "9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb".to_string()),
            },
            simulator_user_id: env::var("SIMULATOR_USER_ID")
                .unwrap_or_else(|_| "63c1d015-6765-4843-9ca3-5ba21ee54d7e".to_string()),
            encryption_secret: env::var("ENCRYPTION_SECRET").map_err(|_| {
                anyhow::anyhow!("ENCRYPTION_SECRET environment variable is required")
            })?,
        })
    }
}
