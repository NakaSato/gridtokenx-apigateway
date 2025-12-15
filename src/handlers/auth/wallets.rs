//! Wallets Handlers Module
//!
//! Wallet and token balance handlers.

use axum::{
    extract::{State, Path},
    Json,
};
use tracing::info;

use crate::AppState;
use super::types::TokenBalanceResponse;

/// Token Balance Handler - queries blockchain for wallet balance
pub async fn token_balance(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
) -> Json<TokenBalanceResponse> {
    info!("💰 Token balance request for wallet: {}", wallet_address);

    // Try to get real balance from blockchain
    let token_balance: f64 = match crate::services::BlockchainService::parse_pubkey(&wallet_address) {
        Ok(wallet_pubkey) => {
            match crate::services::BlockchainService::parse_pubkey(&state.config.energy_token_mint) {
                Ok(mint_pubkey) => {
                    match state.blockchain_service.get_token_balance(&wallet_pubkey, &mint_pubkey).await {
                        Ok(balance) => {
                            let balance_f64 = balance as f64 / 1_000_000_000.0; // Convert from lamports
                            info!("✅ Got real balance from blockchain: {} tokens", balance_f64);
                            balance_f64
                        }
                        Err(e) => {
                            info!("⚠️ Could not get blockchain balance: {}", e);
                            0.0
                        }
                    }
                }
                Err(_) => 0.0
            }
        }
        Err(_) => 0.0
    };

    Json(TokenBalanceResponse {
        wallet_address: wallet_address.clone(),
        token_balance: format!("{:.2}", token_balance),
        token_balance_raw: token_balance,
        balance_sol: 0.0,
        decimals: 9,
        token_mint: state.config.energy_token_mint.clone(),
        token_account: format!("{}...token", &wallet_address[..8.min(wallet_address.len())]),
    })
}
