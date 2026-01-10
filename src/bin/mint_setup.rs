use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::BlockchainService;
use dotenvy::dotenv;
use solana_sdk::signature::Signer;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    
    // Setup logging
    tracing_subscriber::fmt::init();
    
    // Load Engineering Authority
    let authority_path = "../keypairs/dev-wallet.json";
    let authority = BlockchainService::load_keypair_from_file(authority_path)
        .map_err(|e| format!("Failed to load engineering authority from {}: {}", authority_path, e))?;
        
    println!("Authority: {}", authority.pubkey());
    
    // Config
    let rpc_url = env::var("SOLANA_RPC_URL").unwrap_or("http://localhost:8899".to_string());
    let program_config = SolanaProgramsConfig::default(); // Uses env vars or defaults
    
    // Initialize Blockchain Service
    let blockchain = BlockchainService::new(rpc_url, "localnet".to_string(), program_config)
        .expect("Failed to init blockchain service");
        
    // Target User details
    let user_wallet_str = "7VLC6SeuJ6pNFTwd5ifYvK943umUmjgDZ7v9gE9P5zUn";
    let user_wallet = Pubkey::from_str(user_wallet_str)?;
    let mint_str = env::var("ENERGY_TOKEN_MINT").expect("ENERGY_TOKEN_MINT missing");
    println!("Minting to User: {} for Mint: {}", user_wallet, mint_str);
    let _mint = Pubkey::from_str(&mint_str)?;
    
    // Mint 100 Tokens
    let amount_kwh = 100.0;
    
    // Derive ATA (ensure existence done inside mint_energy_tokens?)
    // mint_energy_tokens creates ATA if missing.
    
    // We pass `authority` as signer.
    // We pass dummy values for user_token_account and mint to satisfy signature
    // But mint_energy_tokens internally derives/uses them properly?
    // Let's check signature:
    // pub async fn mint_energy_tokens(&self, authority, _user_token_account, user_wallet, _mint, amount_kwh)
    
    // It ignores _user_token_account and _mint arguments and derives them internally!
    // So we can pass dummies or correct ones.
    
    let dummy = Pubkey::new_unique();
    
    println!("Executing Mint...");
    let sig = blockchain.mint_energy_tokens(
        &authority,
        &dummy, // ignored
        &user_wallet,
        &dummy, // ignored
        amount_kwh
    ).await?;
    
    println!("Mint Success! Signature: {}", sig);
    
    Ok(())
}
