use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::SecureUserInfo;

/// Enhanced registration request with wallet creation option
#[derive(Debug, Deserialize, Serialize, Validate, ToSchema)]
pub struct WalletRegistrationRequest {
    #[validate(length(min = 3, max = 50))]
    pub username: String,

    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8, max = 128))]
    pub password: String,

    #[validate(length(min = 1, max = 20))]
    pub role: String,

    #[validate(length(min = 1, max = 100))]
    pub first_name: String,

    #[validate(length(min = 1, max = 100))]
    pub last_name: String,

    /// Create a new Solana wallet for this user
    pub create_wallet: Option<bool>,

    /// Amount of SOL to airdrop (development only)
    pub airdrop_amount: Option<f64>,

    /// Optional manual wallet address (if not creating new one)
    #[validate(length(min = 32, max = 44))]
    pub wallet_address: Option<String>,
}

/// Response with wallet information for development
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletRegistrationResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: SecureUserInfo,
    pub wallet_info: Option<DevWalletInfo>,
}

/// Development wallet information (DO NOT USE IN PRODUCTION)
#[derive(Debug, Serialize, ToSchema)]
pub struct DevWalletInfo {
    pub address: String,
    pub balance_lamports: u64,
    pub balance_sol: f64,
    pub private_key: String, // Only for development!
    pub airdrop_signature: Option<String>,
    pub created_new: bool,
}

/// Login response with wallet information
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: SecureUserInfo,
    pub wallet_info: Option<UserWalletInfo>,
}

/// Request to export wallet private key
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ExportWalletRequest {
    /// User's current password for re-authentication
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

/// Response containing exported wallet private key
#[derive(Debug, Serialize, ToSchema)]
pub struct ExportWalletResponse {
    /// Private key in Base58 format
    pub private_key: String,
    /// Public key (wallet address)
    pub public_key: String,
    /// Security warning message
    pub warning: String,
}

/// User's wallet information (safe for production)
#[derive(Debug, Serialize, ToSchema)]
pub struct UserWalletInfo {
    pub address: String,
    pub balance_lamports: Option<u64>,
    pub balance_sol: Option<f64>,
}
