// ERC Certificate Lifecycle Integration Test
// Tests the complete lifecycle of Energy Renewable Certificates on-chain
// This test requires a running Solana localnet validator

use anyhow::Result;
use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::{blockchain_service::BlockchainService, erc_service::ErcService};
use chrono::Utc;
use solana_sdk::signature::{Keypair, Signer};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Setup function for ERC lifecycle tests
async fn setup_erc_test() -> Result<(PgPool, Arc<BlockchainService>, ErcService)> {
    // Initialize logging for test visibility
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Connect to test database
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://gridtokenx_user:gridtokenx_password@localhost:5432/gridtokenx".to_string()
    });

    let db_pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Initialize blockchain service (localnet)
    let blockchain_service = Arc::new(
        BlockchainService::new(
            "http://127.0.0.1:8899".to_string(),
            "localnet".to_string(),
            SolanaProgramsConfig::default(),
        )
        .expect("Failed to create blockchain service"),
    );

    // Initialize ERC service
    let erc_service = ErcService::new(db_pool.clone(), (*blockchain_service).clone());

    Ok((db_pool, blockchain_service, erc_service))
}

/// Helper to setup a user and meter with energy generation
async fn setup_meter_with_generation(
    blockchain_service: &BlockchainService,
    authority: &Keypair, // API Gateway authority
    energy_amount: f64,
) -> Result<(Keypair, String)> {
    // 1. Create user keypair and airdrop SOL
    let user_keypair = Keypair::new();
    let pubkey = user_keypair.pubkey();

    println!("   Creating user: {}", pubkey);
    let signature = blockchain_service
        .request_airdrop(&pubkey, 1_000_000_000)
        .await?; // 1 SOL
    blockchain_service
        .wait_for_confirmation(&signature, 30)
        .await?;

    // 2. Register user
    println!("   Registering user on-chain...");
    blockchain_service
        .register_user_on_chain(&user_keypair, 0, "Bangkok")
        .await?;

    // 3. Register meter
    let meter_id = format!("METER-{}", &Uuid::new_v4().to_string()[..8]);
    println!("   Registering meter: {}", meter_id);
    blockchain_service
        .register_meter_on_chain(&user_keypair, &meter_id, 0)
        .await?;

    // 4. Submit reading to generate energy
    println!("   Submitting reading for {} kWh...", energy_amount);
    let produced = (energy_amount * 1000.0) as u64; // Wh
    let consumed = 0;
    let timestamp = Utc::now().timestamp();

    blockchain_service
        .submit_meter_reading_on_chain(
            authority, // Oracle program expects the authority, not user
            &meter_id, produced, consumed, timestamp,
        )
        .await?;

    Ok((user_keypair, meter_id))
}

#[tokio::test]
async fn test_erc_issuance_on_chain() -> Result<()> {
    let (_db_pool, blockchain_service, erc_service) = setup_erc_test().await?;

    println!("\n🏷️ ============================================");
    println!("   Test: ERC Certificate Issuance On-Chain");
    println!("============================================\n");

    // Step 1: Load authority keypair
    println!("📋 Step 1: Load authority keypair");
    let authority = blockchain_service.get_authority_keypair().await?;
    println!("✅ Authority loaded: {}", authority.pubkey());

    // Step 2: Get governance program ID
    println!("\n📋 Step 2: Get governance program ID");
    let governance_program_id = blockchain_service.governance_program_id()?;
    println!("✅ Governance program: {}", governance_program_id);

    // Step 3: Setup user and meter with generation
    println!("\n📋 Step 3: Setup user and meter");
    let energy_amount = 100.0;
    let (user_keypair, meter_id) =
        setup_meter_with_generation(&blockchain_service, &authority, energy_amount).await?;
    println!("✅ User and meter setup complete");

    // Step 4: Issue certificate on-chain
    println!("\n📋 Step 4: Issue ERC certificate on-chain");
    let certificate_id = format!("ERC-TEST-{}", &Uuid::new_v4().to_string()[..8]);
    let renewable_source = "Solar";
    let validation_data = "test_utility_bill_ref_001";

    let tx_signature = erc_service
        .issue_certificate_on_chain(
            &certificate_id,
            &user_keypair.pubkey(),
            &meter_id,
            energy_amount,
            renewable_source,
            validation_data,
            &authority,
            &governance_program_id,
        )
        .await?;

    println!("✅ Certificate issued on-chain");
    println!("   Certificate ID: {}", certificate_id);
    println!("   Transaction: {}", tx_signature);
    println!("   Energy Amount: {} kWh", energy_amount);
    println!("   Source: {}", renewable_source);

    // Step 5: Validate certificate on-chain
    println!("\n📋 Step 5: Validate certificate on-chain");
    let is_valid = erc_service
        .validate_certificate_on_chain(&certificate_id, &governance_program_id)
        .await?;

    println!(
        "✅ Certificate validation: {}",
        if is_valid { "VALID" } else { "INVALID" }
    );
    assert!(is_valid, "Certificate should be valid after issuance");

    println!("\n🎉 ============================================");
    println!("   ERC Issuance Test PASSED");
    println!("============================================\n");

    Ok(())
}

#[tokio::test]
async fn test_erc_transfer_on_chain() -> Result<()> {
    let (_db_pool, blockchain_service, erc_service) = setup_erc_test().await?;

    println!("\n🔄 ============================================");
    println!("   Test: ERC Certificate Transfer On-Chain");
    println!("============================================\n");

    // Step 1: Setup
    println!("📋 Step 1: Setup test environment");
    let authority = blockchain_service.get_authority_keypair().await?;
    let governance_program_id = blockchain_service.governance_program_id()?;

    let energy_amount = 50.0;
    let (from_keypair, meter_id) =
        setup_meter_with_generation(&blockchain_service, &authority, energy_amount).await?;

    let to_wallet = solana_sdk::pubkey::Pubkey::new_unique();

    println!("✅ From wallet: {}", from_keypair.pubkey());
    println!("✅ To wallet: {}", to_wallet);

    // Step 2: Issue initial certificate
    println!("\n📋 Step 2: Issue initial certificate");
    let certificate_id = format!("ERC-TRANSFER-{}", &Uuid::new_v4().to_string()[..8]);

    let issue_tx = erc_service
        .issue_certificate_on_chain(
            &certificate_id,
            &from_keypair.pubkey(),
            &meter_id,
            energy_amount,
            "Wind",
            "transfer_test_validation",
            &authority,
            &governance_program_id,
        )
        .await?;

    println!("✅ Initial certificate issued: {}", issue_tx);

    // Step 3: Transfer certificate
    println!("\n📋 Step 3: Transfer certificate on-chain");
    let transfer_tx = erc_service
        .transfer_certificate_on_chain(
            &certificate_id,
            &from_keypair.pubkey(),
            &to_wallet,
            &authority,
            &governance_program_id,
        )
        .await?;

    println!("✅ Certificate transferred");
    println!("   Transfer transaction: {}", transfer_tx);
    println!("   From: {}", from_keypair.pubkey());
    println!("   To: {}", to_wallet);

    // Step 4: Validate certificate still exists
    println!("\n📋 Step 4: Validate transferred certificate");
    let is_valid = erc_service
        .validate_certificate_on_chain(&certificate_id, &governance_program_id)
        .await?;

    println!("✅ Certificate still valid after transfer: {}", is_valid);
    assert!(is_valid, "Certificate should remain valid after transfer");

    println!("\n🎉 ============================================");
    println!("   ERC Transfer Test PASSED");
    println!("============================================\n");

    Ok(())
}

#[tokio::test]
async fn test_erc_retirement_on_chain() -> Result<()> {
    let (_db_pool, blockchain_service, erc_service) = setup_erc_test().await?;

    println!("\n🗑️ ============================================");
    println!("   Test: ERC Certificate Retirement On-Chain");
    println!("============================================\n");

    // Step 1: Setup
    println!("📋 Step 1: Setup test environment");
    let authority = blockchain_service.get_authority_keypair().await?;
    let governance_program_id = blockchain_service.governance_program_id()?;

    let energy_amount = 75.0;
    let (user_keypair, meter_id) =
        setup_meter_with_generation(&blockchain_service, &authority, energy_amount).await?;

    // Step 2: Issue certificate
    println!("\n📋 Step 2: Issue certificate to retire");
    let certificate_id = format!("ERC-RETIRE-{}", &Uuid::new_v4().to_string()[..8]);

    let issue_tx = erc_service
        .issue_certificate_on_chain(
            &certificate_id,
            &user_keypair.pubkey(),
            &meter_id,
            energy_amount,
            "Hydro",
            "retirement_test_validation",
            &authority,
            &governance_program_id,
        )
        .await?;

    println!("✅ Certificate issued: {}", issue_tx);

    // Step 3: Retire certificate
    println!("\n📋 Step 3: Retire certificate on-chain");
    let retire_tx = erc_service
        .retire_certificate_on_chain(&certificate_id, &authority, &governance_program_id)
        .await?;

    println!("✅ Certificate retired");
    println!("   Retirement transaction: {}", retire_tx);
    println!("   Certificate ID: {}", certificate_id);

    println!("\n🎉 ============================================");
    println!("   ERC Retirement Test PASSED");
    println!("============================================\n");

    Ok(())
}

#[tokio::test]
async fn test_complete_erc_lifecycle() -> Result<()> {
    let (_db_pool, blockchain_service, erc_service) = setup_erc_test().await?;

    println!("\n♻️ ============================================");
    println!("   Test: Complete ERC Lifecycle");
    println!("   (Issue → Transfer → Retire)");
    println!("============================================\n");

    // Setup
    let authority = blockchain_service.get_authority_keypair().await?;
    let governance_program_id = blockchain_service.governance_program_id()?;

    let energy_amount = 200.0;
    let (original_owner_keypair, meter_id) =
        setup_meter_with_generation(&blockchain_service, &authority, energy_amount).await?;

    let new_owner = solana_sdk::pubkey::Pubkey::new_unique();

    // Use a shorter ID to satisfy Solana seed limit (32 bytes)
    let certificate_id = format!("ERC-LIFECYCLE-{}", &Uuid::new_v4().to_string()[..8]);

    // Phase 1: Issuance
    println!("📋 Phase 1: Certificate Issuance");
    let issue_tx = erc_service
        .issue_certificate_on_chain(
            &certificate_id,
            &original_owner_keypair.pubkey(),
            &meter_id,
            energy_amount,
            "Solar",
            "lifecycle_test_validation",
            &authority,
            &governance_program_id,
        )
        .await?;
    println!("✅ Issued: {}", issue_tx);

    // Validate after issuance
    let valid_after_issue = erc_service
        .validate_certificate_on_chain(&certificate_id, &governance_program_id)
        .await?;
    assert!(
        valid_after_issue,
        "Certificate should be valid after issuance"
    );
    println!("✅ Validated after issuance");

    // Phase 2: Transfer
    println!("\n📋 Phase 2: Certificate Transfer");
    let transfer_tx = erc_service
        .transfer_certificate_on_chain(
            &certificate_id,
            &original_owner_keypair.pubkey(),
            &new_owner,
            &authority,
            &governance_program_id,
        )
        .await?;
    println!("✅ Transferred: {}", transfer_tx);

    // Validate after transfer
    let valid_after_transfer = erc_service
        .validate_certificate_on_chain(&certificate_id, &governance_program_id)
        .await?;
    assert!(
        valid_after_transfer,
        "Certificate should be valid after transfer"
    );
    println!("✅ Validated after transfer");

    // Phase 3: Retirement
    println!("\n📋 Phase 3: Certificate Retirement");
    let retire_tx = erc_service
        .retire_certificate_on_chain(&certificate_id, &authority, &governance_program_id)
        .await?;
    println!("✅ Retired: {}", retire_tx);

    println!("\n📊 Lifecycle Summary:");
    println!("   Certificate ID: {}", certificate_id);
    println!("   Original Owner: {}", original_owner_keypair.pubkey());
    println!("   New Owner: {}", new_owner);
    println!("   Energy Amount: 200.0 kWh");
    println!("   Source: Solar");
    println!("   Issue TX: {}", issue_tx);
    println!("   Transfer TX: {}", transfer_tx);
    println!("   Retire TX: {}", retire_tx);

    println!("\n🎉 ============================================");
    println!("   Complete ERC Lifecycle Test PASSED");
    println!("============================================\n");

    Ok(())
}

#[tokio::test]
async fn test_erc_metadata_creation() -> Result<()> {
    let (_db_pool, _blockchain_service, erc_service) = setup_erc_test().await?;

    println!("\n📝 ============================================");
    println!("   Test: ERC Metadata Creation");
    println!("============================================\n");

    // Use a shorter ID to satisfy Solana seed limit (32 bytes)
    let certificate_id = format!("ERC-META-{}", &Uuid::new_v4().to_string()[..8]);
    let energy_amount = 150.0;
    let renewable_source = "Wind";
    let issuer = "GridTokenX Test Issuer";
    let issue_date = Utc::now();
    let expiry_date = Some(issue_date + chrono::Duration::days(365));
    let validation_data = "metadata_test_validation_ref";

    println!("📋 Creating ERC metadata");
    let metadata = erc_service.create_certificate_metadata(
        &certificate_id,
        energy_amount,
        renewable_source,
        issuer,
        issue_date,
        expiry_date,
        validation_data,
    )?;

    println!("✅ Metadata created successfully");
    println!("   Name: {}", metadata.name);
    println!("   Description: {}", metadata.description);

    // Verify metadata structure
    assert_eq!(
        metadata.name,
        format!("Renewable Energy Certificate #{}", certificate_id)
    );
    assert!(!metadata.description.is_empty());
    assert!(!metadata.attributes.is_empty());

    // Verify attributes contain expected data
    let has_energy_attr = metadata
        .attributes
        .iter()
        .any(|attr| attr.trait_type == "Energy Amount");
    assert!(
        has_energy_attr,
        "Metadata should contain energy amount attribute"
    );

    let has_source_attr = metadata
        .attributes
        .iter()
        .any(|attr| attr.trait_type == "Renewable Source");
    assert!(
        has_source_attr,
        "Metadata should contain renewable source attribute"
    );

    println!("✅ Metadata structure validated");

    println!("\n🎉 ============================================");
    println!("   ERC Metadata Test PASSED");
    println!("============================================\n");

    Ok(())
}
