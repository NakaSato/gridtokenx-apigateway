// Token Minting Integration Test
// Tests actual Energy Token (SPL Token) operations on Solana blockchain
// using the BlockchainService and TokenManager.

use anyhow::Result;
use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::blockchain::BlockchainService;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
// use std::str::FromStr; // Removed unused import
use std::sync::Arc;
// use tokio::time::Duration; // Removed unused import

/// Setup function for token minting tests with Robust Funding logic
async fn setup_token_test() -> Result<Arc<BlockchainService>> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Initialize blockchain service (localnet)
    // Ensure we are using localhost URL
    let blockchain_service = Arc::new(
        BlockchainService::new(
            "http://127.0.0.1:8899".to_string(),
            "localnet".to_string(),
            SolanaProgramsConfig::default(),
        )
        .expect("Failed to create blockchain service"),
    );

    // Load authority
    let authority = blockchain_service.get_authority_keypair().await?;
    println!("   Authority: {}", authority.pubkey());

    // Ensure authority has enough SOL (Self-Funding Logic)
    let balance = blockchain_service.get_balance_sol(&authority.pubkey()).await.unwrap_or(0.0);
    if balance < 1.0 {
        println!("   Authority has low balance ({:.2} SOL). Airdropping...", balance);
        match blockchain_service.request_airdrop(&authority.pubkey(), 5_000_000_000).await {
            Ok(sig) => {
                 println!("   Airdrop requested. Signature: {}", sig);
                 blockchain_service.wait_for_confirmation(&sig, 30).await?;
            },
            Err(e) => {
                println!("   Airdrop failed: {}. Assuming manual funding or race condition.", e);
            }
        }
    }
    
    // Double check balance
    let final_balance = blockchain_service.get_balance_sol(&authority.pubkey()).await.unwrap_or(0.0);
    println!("   Final Authority Balance: {:.2} SOL", final_balance);

    // Initialize Energy Token Program (Mint) if needed
    let energy_token_program_id = blockchain_service.energy_token_program_id()?;
    let (mint_pda, _) = Pubkey::find_program_address(&[b"mint"], &energy_token_program_id);
    
    println!("   Checking Mint PDA: {}", mint_pda);
    let exists = blockchain_service.account_exists(&mint_pda).await?;
    println!("   Mint Exists: {}", exists);

    if !exists {
        println!("   Initializing Energy Token Program Mint...");
        match blockchain_service.initialize_energy_token(&authority).await {
            Ok(sig) => {
                println!("   Initialization successful: {}", sig);
                blockchain_service.wait_for_confirmation(&sig, 30).await?;
            },
            Err(e) => {
                println!("   Initialization failed (might be already initialized elsewhere): {}", e);
            }
        }
    } else {
        println!("   Energy Token Mint already initialized.");
    }

    Ok(blockchain_service)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_complete_token_lifecycle() -> Result<()> {
    let blockchain_service = setup_token_test().await?;

    println!("\n♻️ ============================================");
    println!("   Test: Complete Energy Token Lifecycle (REAL)");
    println!("   (Mint [via Anchor] → Transfer → Burn)");
    println!("============================================\n");

    // 1. Setup Identities
    let authority = blockchain_service.get_authority_keypair().await?;
    let user_keypair = Keypair::new();
    let recipient_keypair = Keypair::new();

    println!("📋 Identities:");
    println!("   Authority: {}", authority.pubkey());
    println!("   User:      {}", user_keypair.pubkey());
    println!("   Recipient: {}", recipient_keypair.pubkey());

    // Fund User and Recipient for transactions
    println!("   Funding User and Recipient...");
    let sig1 = blockchain_service.request_airdrop(&user_keypair.pubkey(), 1_000_000_000).await?;
    let sig2 = blockchain_service.request_airdrop(&recipient_keypair.pubkey(), 1_000_000_000).await?;
    blockchain_service.wait_for_confirmation(&sig1, 30).await?;
    blockchain_service.wait_for_confirmation(&sig2, 30).await?;

    // 2. Derive Program Addresses (Mint, ATAs)
    let energy_token_program_id = blockchain_service.energy_token_program_id()?;
    let (mint_pda, _) = Pubkey::find_program_address(&[b"mint"], &energy_token_program_id);
    
    // Note: Token Manager derives ATAs internally for minting, but we need them for transfer verification
    // Use Standard Token Program
    let token_program_id = spl_token::id();
    
    // Use standard helper for ATAs
    let user_ata = spl_associated_token_account::get_associated_token_address(
        &user_keypair.pubkey(),
        &mint_pda,
    );
    let recipient_ata = spl_associated_token_account::get_associated_token_address(
        &recipient_keypair.pubkey(),
        &mint_pda,
    );

    println!("\n📋 Derived Addresses:");
    println!("   Program ID: {}", energy_token_program_id);
    println!("   Token Prog: {}", token_program_id);
    println!("   Mint PDA:   {}", mint_pda);
    println!("   User ATA:   {}", user_ata);
    println!("   Recip ATA:  {}", recipient_ata);

    // 3. Mint Tokens (e.g., 1000 kWh)
    println!("\n📋 Phase 1: Mint 1000 kWh to User");
    // TokenManager::mint_energy_tokens handles ATA creation if needed (now using Token-2022 via updated Utils)
    
    let mint_sig = blockchain_service.mint_energy_tokens(
        &authority,
        &user_ata,
        &user_keypair.pubkey(),
        &mint_pda,
        1000.0
    ).await?;
    
    println!("   Mint Tx: {}", mint_sig);
    blockchain_service.wait_for_confirmation(&mint_sig, 30).await?;

    // Verify Balance
    // NOTE: blockchain_service.get_token_balance calls token_manager.get_token_balance
    // which calls account_manager.calculate_ata_address
    // I need to ensure account_manager calculates ATA using Token-2022?
    // Let's rely on standard logic. If it fails, I might need to fix account_manager too.
    
    let user_balance = blockchain_service.get_token_balance(&user_keypair.pubkey(), &mint_pda).await?;
    println!("   User Balance: {} (raw units)", user_balance);
    // 1000.0 * 1_000_000_000 = 1,000,000,000,000
    assert_eq!(user_balance, 1_000_000_000_000, "User should have 1000 kWh minted");
    println!("✅ Mint successful");

    // 4. Transfer Tokens (e.g., 400 kWh) to Recipient
    println!("\n📋 Phase 2: Transfer 400 kWh to Recipient");
    
    println!("   Ensuring Recipient ATA exists (via Create Instruction)...");
    
    // Build CreateIdempotent instruction
    let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
        &authority.pubkey(), // Payer (Main Authority has SOL)
        &recipient_keypair.pubkey(), // Wallet
        &mint_pda,
        &token_program_id,
    );
    
    // We transaction with Authority as payer
    let create_sig = blockchain_service.build_and_send_transaction(
        vec![create_ata_ix],
        &[&authority]
    ).await?;
    
    println!("   Create ATA Tx: {}", create_sig);
    blockchain_service.wait_for_confirmation(&create_sig, 30).await?;

    let transfer_sig = blockchain_service.token_manager.transfer_energy_tokens(
        &user_keypair,      // User signs as owner and payer
        &user_ata,          // From
        &recipient_ata,     // To
        &mint_pda,          // Mint
        400.0               // Amount
    ).await?;

    println!("   Transfer Tx: {}", transfer_sig);
    blockchain_service.wait_for_confirmation(&transfer_sig, 30).await?;

    // Verify Balances
    let user_balance_2 = blockchain_service.get_token_balance(&user_keypair.pubkey(), &mint_pda).await?;
    let recip_balance = blockchain_service.get_token_balance(&recipient_keypair.pubkey(), &mint_pda).await?;

    println!("   User Balance:      {}", user_balance_2);
    println!("   Recipient Balance: {}", recip_balance);

    assert_eq!(user_balance_2, 600_000_000_000, "User should have 600 kWh remaining");
    assert_eq!(recip_balance, 400_000_000_000, "Recipient should have 400 kWh");
    println!("✅ Transfer successful");

    // 5. Burn Tokens (Burn 200 kWh from Recipient)
    println!("\n📋 Phase 3: Burn 200 kWh from Recipient");
    
    // Burn 200 kWh from Recipient
    let burn_sig = blockchain_service.token_manager.burn_energy_tokens(
        &recipient_keypair, // Recipient signs as owner
        &recipient_ata,
        &mint_pda,
        200.0 
    ).await?;

    println!("   Burn Tx: {}", burn_sig);
    blockchain_service.wait_for_confirmation(&burn_sig, 30).await?;

    // Verify Balance
    let recip_balance_final = blockchain_service.get_token_balance(&recipient_keypair.pubkey(), &mint_pda).await?;
    println!("   Recipient Final Balance: {}", recip_balance_final);
    
    assert_eq!(recip_balance_final, 200_000_000_000, "Recipient should have 200 kWh remaining (400 - 200)");
    println!("✅ Burn successful");

    println!("\n🎉 ============================================");
    println!("   Complete Energy Token Lifecycle Test PASSED");
    println!("============================================\n");

    Ok(())
}
