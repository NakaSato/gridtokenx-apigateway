use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::BlockchainService;
use dotenvy::dotenv;
use solana_sdk::signature::Signer;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    
    // Setup logging
    tracing_subscriber::fmt::init();
    
    // Load Authority
    let authority_path = std::env::var("AUTHORITY_WALLET_PATH").unwrap_or("../keypairs/dev-wallet.json".to_string());
    println!("Loading authority from: {}", authority_path);
    let authority = BlockchainService::load_keypair_from_file(&authority_path)
        .expect("Failed to load authority");
        
    println!("Authority: {}", authority.pubkey());
    
    // Config
    let rpc_url = env::var("SOLANA_RPC_URL").unwrap_or("http://localhost:8899".to_string());
    let program_config = SolanaProgramsConfig {
        registry_program_id: env::var("SOLANA_REGISTRY_PROGRAM_ID")
            .unwrap_or_else(|_| "HWoKSbNy4jJBFJ7g7drxZgAfTmjFqvg1Sx6vXosfJNAi".to_string()),
        oracle_program_id: env::var("SOLANA_ORACLE_PROGRAM_ID")
            .unwrap_or_else(|_| "5z6Qaf6UUv42uCqbxQLfKz7cSXhMABsq73mRMwvHKzFA".to_string()),
        governance_program_id: env::var("SOLANA_GOVERNANCE_PROGRAM_ID")
            .unwrap_or_else(|_| "2WrMSfreZvCCKdQMQGY7bTFgXKgr42fYipJR6VXn1Q8c".to_string()),
        energy_token_program_id: env::var("SOLANA_ENERGY_TOKEN_PROGRAM_ID")
            .unwrap_or_else(|_| "MwAdshY2978VqcpJzWSKmPfDtKfweD7YLMCQSBcR4wP".to_string()),
        trading_program_id: env::var("SOLANA_TRADING_PROGRAM_ID")
            .unwrap_or_else(|_| "Fmk6vb74MjZpXVE9kAS5q4U5L8hr2AEJcDikfRSFTiyY".to_string()),
    };
    
    // Initialize Blockchain Service
    let blockchain = BlockchainService::new(rpc_url.clone(), "localnet".to_string(), program_config)
        .expect("Failed to init blockchain service");

    println!("Initializing Blockchain on {}...", rpc_url);

    // 1. Initialize Registry
    println!("1. Initializing Registry...");
    match blockchain.initialize_registry(&authority).await {
        Ok(sig) => println!("   Success: {}", sig),
        Err(e) => println!("   Failed (maybe already init): {}", e),
    }

    // 2. Initialize Oracle
    // Note: We need API Gateway Pubkey. Using Authority as API Gateway for testing.
    println!("2. Initializing Oracle...");
    match blockchain.initialize_oracle(&authority, &authority.pubkey()).await {
        Ok(sig) => println!("   Success: {}", sig),
        Err(e) => println!("   Failed (maybe already init): {}", e),
    }

    // 3. Initialize Governance
    println!("3. Initializing Governance...");
    match blockchain.initialize_governance(&authority).await {
        Ok(sig) => println!("   Success: {}", sig),
        Err(e) => println!("   Failed (maybe already init): {}", e),
    }

    // 4. Initialize Energy Token
    println!("4. Initializing Energy Token Mint...");
    match blockchain.initialize_energy_token(&authority).await {
        Ok(sig) => println!("   Success: {}", sig),
        Err(e) => println!("   Failed (maybe already init): {}", e),
    }

    // 5. Initialize Trading Market
    println!("5. Initializing Trading Market...");
    match blockchain.initialize_trading_market(&authority).await {
        Ok(sig) => println!("   Success: {}", sig),
        Err(e) => println!("   Failed (maybe already init): {}", e),
    }
    
    println!("Blockchain Initialization Complete.");
    Ok(())
}
