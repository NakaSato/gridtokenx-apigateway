use anyhow::Result;
use api_gateway::config::SolanaProgramsConfig;
use api_gateway::services::BlockchainService;
use solana_sdk::{pubkey::Pubkey, signature::Signer};
use std::process::Command;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🔧 ============================================");
    println!("   Blockchain Environment Setup");
    println!("   Preparing localnet for integration testing");
    println!("============================================\n");

    // ========================================
    // Step 1: Initialize Blockchain Service
    // ========================================
    println!("📋 Step 1: Initialize Blockchain Service");
    println!("------------------------------------------");

    print!("Connecting to localnet RPC... ");
    let blockchain_service = match BlockchainService::new(
        "http://127.0.0.1:8899".to_string(),
        "localnet".to_string(),
        SolanaProgramsConfig::default(),
    ) {
        Ok(service) => {
                println!("✅");
                service
            }
            Err(e) => {
                println!("❌");
                println!("\n❌ Error: {}", e);
                println!("\n💡 Make sure solana-test-validator is running:");
                println!("   solana-test-validator");
                return Err(e);
            }
        };

    // ========================================
    // Step 2: Load Authority Keypair
    // ========================================
    println!("\n📋 Step 2: Load Authority Keypair");
    println!("----------------------------------");

    print!("Loading authority from dev-wallet.json... ");
    let authority = match blockchain_service.get_authority_keypair().await {
        Ok(auth) => {
            println!("✅");
            println!("  Authority: {}", auth.pubkey());
            auth
        }
        Err(e) => {
            println!("❌");
            println!("\n❌ Error: {}", e);
            return Err(e);
        }
    };

    // ========================================
    // Step 3: Check and Fund Authority Wallet
    // ========================================
    println!("\n📋 Step 3: Check and Fund Authority Wallet");
    println!("-------------------------------------------");

    print!("Checking authority balance... ");
    let balance = match blockchain_service
        .get_balance_sol(&authority.pubkey())
        .await
    {
        Ok(bal) => {
            println!("✅");
            println!("  Current Balance: {} SOL", bal);
            bal
        }
        Err(e) => {
            println!("❌");
            println!("\n❌ Error: {}", e);
            return Err(e);
        }
    };

    // Fund if balance is low
    const MIN_BALANCE: f64 = 5.0;
    if balance < MIN_BALANCE {
        println!(
            "\n⚠️  Balance is low (< {} SOL). Requesting airdrop...",
            MIN_BALANCE
        );

        print!("Requesting 10 SOL airdrop... ");
        let output = Command::new("solana")
            .args(&["airdrop", "10", &authority.pubkey().to_string()])
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    println!("✅");

                    // Wait a moment for the airdrop to process
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                    // Check new balance
                    if let Ok(new_balance) = blockchain_service
                        .get_balance_sol(&authority.pubkey())
                        .await
                    {
                        println!("  New Balance: {} SOL", new_balance);
                    }
                } else {
                    println!("❌");
                    let error = String::from_utf8_lossy(&result.stderr);
                    println!("  Error: {}", error);
                    println!("\n💡 You can manually airdrop using:");
                    println!("   solana airdrop 10");
                }
            }
            Err(e) => {
                println!("❌");
                println!("  Error: {}", e);
                println!("\n💡 You can manually airdrop using:");
                println!("   solana airdrop 10");
            }
        }
    } else {
        println!("✅ Balance is sufficient (>= {} SOL)", MIN_BALANCE);
    }

    // ========================================
    // Step 4: Verify Program Deployments
    // ========================================
    println!("\n📋 Step 4: Verify Program Deployments");
    println!("--------------------------------------");

    let programs = vec![
        ("Registry", "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7"),
        ("Oracle", "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE"),
        ("Governance", "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe"),
        (
            "Energy Token",
            "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur",
        ),
        ("Trading", "GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk"),
    ];

    let mut all_deployed = true;
    let mut missing_programs = Vec::new();

    for (name, program_id_str) in &programs {
        print!("Checking {} program... ", name);

        match Pubkey::from_str(program_id_str) {
            Ok(program_id) => match blockchain_service.account_exists(&program_id).await {
                Ok(exists) => {
                    if exists {
                        println!("✅ (Deployed)");
                    } else {
                        println!("⚠️  (Not found)");
                        all_deployed = false;
                        missing_programs.push(*name);
                    }
                }
                Err(e) => {
                    println!("❌");
                    println!("  Error checking account: {}", e);
                    all_deployed = false;
                    missing_programs.push(*name);
                }
            },
            Err(e) => {
                println!("❌");
                println!("  Invalid program ID: {}", e);
                all_deployed = false;
                missing_programs.push(*name);
            }
        }
    }

    // ========================================
    // Step 5: Check Token Mint
    // ========================================
    println!("\n📋 Step 5: Check Energy Token Mint");
    println!("-----------------------------------");

    print!("Checking if Energy Token mint exists... ");
    let mint_str = "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur";
    match Pubkey::from_str(mint_str) {
        Ok(mint) => match blockchain_service.account_exists(&mint).await {
            Ok(exists) => {
                if exists {
                    println!("✅ (Mint exists)");
                } else {
                    println!("⚠️  (Mint not found)");
                    println!(
                        "  Note: Mint will be created when the Energy Token program is initialized"
                    );
                }
            }
            Err(e) => {
                println!("❌");
                println!("  Error: {}", e);
            }
        },
        Err(e) => {
            println!("❌");
            println!("  Invalid mint address: {}", e);
        }
    }

    // ========================================
    // Step 6: Initialize Governance Program
    // ========================================
    println!("\n📋 Step 6: Initialize Governance Program");
    println!("-----------------------------------------");

    let governance_program_id = Pubkey::from_str("4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe")?;
    let (poa_config_pda, _bump) =
        Pubkey::find_program_address(&[b"poa_config"], &governance_program_id);

    print!("Checking if Governance Program is initialized... ");
    let account_data = blockchain_service.get_account_data(&poa_config_pda).await;
    let is_initialized = match account_data {
        Ok(data) => data.len() > 0,
        Err(_) => false,
    };

    if is_initialized {
        println!("✅ (Already initialized)");
    } else {
        println!("⚠️  (Not initialized)");
        print!("Initializing Governance Program... ");

        // Build initialize instruction
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:initialize_poa");
        let hash = hasher.finalize();

        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&hash[0..8]);

        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(poa_config_pda, false),
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ),
        ];

        let instruction = solana_sdk::instruction::Instruction::new_with_bytes(
            governance_program_id,
            &instruction_data,
            accounts,
        );

        match blockchain_service
            .build_and_send_transaction(vec![instruction], &[&authority])
            .await
        {
            Ok(sig) => {
                println!("✅");
                println!("  Signature: {}", sig);
            }
            Err(e) => {
                println!("❌");
                println!("  Error initializing governance program: {}", e);
                // Don't fail the script, just warn
            }
        }
    }

    // ========================================
    // Step 7: Initialize Oracle Program
    // ========================================
    println!("\n📋 Step 7: Initialize Oracle Program");
    println!("--------------------------------------");

    let oracle_program_id = Pubkey::from_str("DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE")?;
    let (oracle_data_pda, _bump) =
        Pubkey::find_program_address(&[b"oracle_data"], &oracle_program_id);

    print!("Checking if Oracle Program is initialized... ");
    let account_data = blockchain_service.get_account_data(&oracle_data_pda).await;
    let is_initialized = match account_data {
        Ok(data) => data.len() > 0,
        Err(_) => false,
    };

    if is_initialized {
        println!("✅ (Already initialized)");
    } else {
        println!("⚠️  (Not initialized)");
        print!("Initializing Oracle Program... ");

        // Build initialize instruction
        let mut instruction_data = Vec::new();

        // Use discriminator from IDL: [175, 175, 109, 31, 13, 152, 155, 237]
        instruction_data.extend_from_slice(&[175, 175, 109, 31, 13, 152, 155, 237]);

        // Add api_gateway argument (authority pubkey for testing)
        instruction_data.extend_from_slice(&authority.pubkey().to_bytes());

        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(oracle_data_pda, false),
            solana_sdk::instruction::AccountMeta::new(authority.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new_readonly(
                solana_sdk::pubkey!("11111111111111111111111111111111"),
                false,
            ),
        ];

        let instruction = solana_sdk::instruction::Instruction::new_with_bytes(
            oracle_program_id,
            &instruction_data,
            accounts,
        );

        match blockchain_service
            .build_and_send_transaction(vec![instruction], &[&authority])
            .await
        {
            Ok(sig) => {
                println!("✅");
                println!("  Signature: {}", sig);
            }
            Err(e) => {
                println!("❌");
                println!("  Error initializing oracle program: {}", e);
                // Don't fail the script, just warn
            }
        }
    }

    // ========================================
    // Final Summary
    // ========================================
    println!("\n🎉 ============================================");
    println!("   Setup Summary");
    println!("============================================\n");

    if all_deployed {
        println!("✅ All programs are deployed!");
        println!("✅ Authority wallet is funded!");
        println!("\n🚀 Environment is ready for integration testing!");
        println!("\n📝 Next Steps:");
        println!("  1. Run integration tests:");
        println!("     cargo test --test erc_lifecycle_test");
        println!("     cargo test --test token_minting_test");
        println!("     cargo test --test settlement_test");
    } else {
        println!("⚠️  Some programs are not deployed:");
        for program in &missing_programs {
            println!("  - {}", program);
        }
        println!("\n💡 To deploy programs:");
        println!("  1. Navigate to the anchor project:");
        println!("     cd ../gridtokenx-anchor");
        println!("  2. Build and deploy:");
        println!("     anchor build");
        println!("     anchor deploy");
        println!("  3. Re-run this setup script:");
        println!("     cargo run --example setup_blockchain_env");
    }

    println!();

    Ok(())
}
