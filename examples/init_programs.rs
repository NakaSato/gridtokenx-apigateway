use anyhow::Result;
use solana_sdk::signature::{Keypair, Signer};
use api_gateway::services::blockchain::instructions::InstructionBuilder;
use api_gateway::services::blockchain::transactions::TransactionHandler;
use std::sync::Arc;
use solana_client::rpc_client::RpcClient;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let rpc_url = std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://localhost:8899".to_string());
    println!("Connecting to RPC: {}", rpc_url);
    let rpc_client = Arc::new(RpcClient::new_with_commitment(rpc_url, solana_sdk::commitment_config::CommitmentConfig::confirmed()));
    let handler = TransactionHandler::new(rpc_client.clone());

    // Load authority
    let wallet_path = "dev-wallet.json";
    let bytes = std::fs::read_to_string(wallet_path)?;
    let wallet_data: Vec<u8> = serde_json::from_str(&bytes)?;
    let authority = Keypair::try_from(&wallet_data[..]).map_err(|e| anyhow::anyhow!("Invalid keypair: {:?}", e))?;
    
    // Check balance with retry
    let mut balance = 0;
    for _ in 0..30 {
        balance = rpc_client.get_balance(&authority.pubkey())?;
        if balance > 0 { break; }
        println!("⏳ Waiting for balance to reflect on {}...", authority.pubkey());
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    println!("Authority: {} - Balance: {} lamports", authority.pubkey(), balance);
    
    if balance == 0 {
        return Err(anyhow::anyhow!("Authority has no balance. Airdrop failed."));
    }
    
    let builder = InstructionBuilder::new(authority.pubkey());
    
    println!("Initializing Trading Market...");
    let init_market_ix = builder.build_initialize_market_instruction(authority.pubkey())?;
    
    // We try to send. If it fails because it's already initialized, we continue.
    match handler.build_and_send_transaction(vec![init_market_ix], &[&authority]).await {
        Ok(sig) => println!("✅ Market initialized: {}", sig),
        Err(e) => println!("⚠️  Market init skipped/failed (likely already exists): {}", e),
    }

    println!("Initializing Energy Token Program...");
    let init_token_ix = builder.build_initialize_energy_token_instruction(authority.pubkey())?;
    match handler.build_and_send_transaction(vec![init_token_ix], &[&authority]).await {
        Ok(sig) => println!("✅ Energy Token initialized: {}", sig),
        Err(e) => println!("⚠️  Energy Token init skipped/failed (likely already exists): {}", e),
    }

    Ok(())
}
