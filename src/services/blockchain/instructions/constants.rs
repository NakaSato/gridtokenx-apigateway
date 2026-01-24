use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// System program ID constant
pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";

/// Program IDs (localnet) — keep in sync with `gridtokenx-anchor/Anchor.toml`
pub const REGISTRY_PROGRAM_ID: &str = "HWoKSbNy4jJBFJ7g7drxZgAfTmjFqvg1Sx6vXosfJNAi";
pub const ORACLE_PROGRAM_ID: &str = "5z6Qaf6UUv42uCqbxQLfKz7cSXhMABsq73mRMwvHKzFA";
pub const GOVERNANCE_PROGRAM_ID: &str = "2WrMSfreZvCCKdQMQGY7bTFgXKgr42fYipJR6VXn1Q8c";
pub const ENERGY_TOKEN_PROGRAM_ID: &str = "MwAdshY2978VqcpJzWSKmPfDtKfweD7YLMCQSBcR4wP";
pub const TRADING_PROGRAM_ID: &str = "Fmk6vb74MjZpXVE9kAS5q4U5L8hr2AEJcDikfRSFTiyY";

/// Program ID utilities
pub mod program_ids {
    use super::*;
    use anyhow::{anyhow, Result};

    pub fn registry_program_id() -> Result<Pubkey> {
        Pubkey::from_str(REGISTRY_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse registry program ID: {}", e))
    }

    pub fn oracle_program_id() -> Result<Pubkey> {
        Pubkey::from_str(ORACLE_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse oracle program ID: {}", e))
    }

    pub fn governance_program_id() -> Result<Pubkey> {
        Pubkey::from_str(GOVERNANCE_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse governance program ID: {}", e))
    }

    pub fn energy_token_program_id() -> Result<Pubkey> {
        Pubkey::from_str(ENERGY_TOKEN_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse energy token program ID: {}", e))
    }

    pub fn trading_program_id() -> Result<Pubkey> {
        Pubkey::from_str(TRADING_PROGRAM_ID)
            .map_err(|e| anyhow!("Failed to parse trading program ID: {}", e))
    }
}
