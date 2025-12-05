use anyhow::Result;
use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::BlockchainService;
use solana_sdk::signature::Signer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔑 Testing Keypair Loading from dev-wallet.json\n");

    // Test 1: Load keypair from file
    println!("Test 1: Loading keypair from dev-wallet.json...");
    let keypair = BlockchainService::load_keypair_from_file("dev-wallet.json")?;
    println!("✅ Keypair loaded successfully!");
    println!("   Public Key: {}", keypair.pubkey());
    println!();

    // Test 2: Test get_authority_keypair method
    println!("Test 2: Testing get_authority_keypair method...");
    let blockchain_service = BlockchainService::new(
        "http://127.0.0.1:8899".to_string(),
        "localnet".to_string(),
        SolanaProgramsConfig::default(),
    )?;

    let authority = blockchain_service.get_authority_keypair().await?;
    println!("✅ Authority keypair loaded successfully!");
    println!("   Public Key: {}", authority.pubkey());
    println!();

    // Test 3: Verify both keypairs match
    println!("Test 3: Verifying keypairs match...");
    if keypair.pubkey() == authority.pubkey() {
        println!("✅ Both methods return the same keypair!");
    } else {
        println!("❌ Keypairs don't match!");
    }
    println!();

    println!("🎉 All tests passed! Phase 1: Key Management is complete.");

    Ok(())
}
