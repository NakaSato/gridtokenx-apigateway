//! CLI tool to fix user wallets with encryption issues
//!
//! Usage:
//!   cargo run --example fix_user_wallets -- [OPTIONS]
//!
//! Options:
//!   --diagnose          Only diagnose, don't fix
//!   --user <email>      Fix specific user by email
//!   --all               Fix all users with issues
//!   --force             Force regenerate even valid wallets

use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::time::Duration;

// Import the services
use api_gateway::services::blockchain_service::BlockchainService;
use api_gateway::services::wallet_initialization_service::{
    WalletInitializationService, WalletStatus,
};

#[derive(Parser, Debug)]
#[command(name = "fix_user_wallets")]
#[command(about = "Fix user wallet encryption issues")]
struct Args {
    /// Only diagnose wallets, don't fix them
    #[arg(short, long)]
    diagnose: bool,

    /// Fix a specific user by email
    #[arg(short, long)]
    user: Option<String>,

    /// Fix all users with wallet issues
    #[arg(short, long)]
    all: bool,

    /// Force regenerate wallets even if they're valid
    #[arg(short, long)]
    force: bool,

    /// Database URL (defaults to DATABASE_URL env var)
    #[arg(long)]
    database_url: Option<String>,

    /// Encryption secret (defaults to ENCRYPTION_SECRET env var)
    #[arg(long)]
    encryption_secret: Option<String>,

    /// Solana RPC URL (defaults to SOLANA_RPC_URL env var)
    #[arg(long)]
    solana_rpc_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("api_gateway=info".parse().unwrap())
                .add_directive("fix_user_wallets=info".parse().unwrap()),
        )
        .init();

    // Load .env file if present
    dotenvy::dotenv().ok();

    let args = Args::parse();

    // Get configuration
    let database_url = args
        .database_url
        .or_else(|| env::var("DATABASE_URL").ok())
        .expect("DATABASE_URL must be set");

    let encryption_secret = args
        .encryption_secret
        .or_else(|| env::var("ENCRYPTION_SECRET").ok())
        .expect("ENCRYPTION_SECRET must be set");

    let solana_rpc_url = args
        .solana_rpc_url
        .or_else(|| env::var("SOLANA_RPC_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:8899".to_string());

    println!("🔧 Wallet Encryption Fix Tool");
    println!("==============================\n");
    println!("Database: {}", database_url.split('@').last().unwrap_or(&database_url));
    println!("Solana RPC: {}", solana_rpc_url);
    println!();

    // Connect to database
    let db = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await?;

    println!("✅ Connected to database\n");

    // Create blockchain service
    let blockchain_service = BlockchainService::new(solana_rpc_url.clone(), "localnet".to_string())?;

    // Create wallet initialization service
    let service = WalletInitializationService::new(
        db,
        encryption_secret,
        blockchain_service,
        solana_rpc_url,
    );

    if args.diagnose {
        // Diagnose only
        println!("📊 Diagnosing all user wallets...\n");
        let diagnoses = service.diagnose_all_users().await?;

        let mut no_wallet = 0;
        let mut address_only = 0;
        let mut valid = 0;
        let mut legacy = 0;
        let mut corrupted = 0;

        println!("{:<36} {:<20} {:<20} {:<10}", "User ID", "Email", "Status", "Can Decrypt");
        println!("{}", "-".repeat(90));

        for diagnosis in &diagnoses {
            let status_str = match &diagnosis.status {
                WalletStatus::NoWallet => {
                    no_wallet += 1;
                    "No Wallet"
                }
                WalletStatus::AddressOnlyNoKeys => {
                    address_only += 1;
                    "Address Only"
                }
                WalletStatus::ValidEncryption => {
                    valid += 1;
                    "Valid"
                }
                WalletStatus::LegacyEncryption => {
                    legacy += 1;
                    "Legacy (16-byte IV)"
                }
                WalletStatus::CorruptedEncryption => {
                    corrupted += 1;
                    "Corrupted"
                }
            };

            let can_decrypt = if diagnosis.can_decrypt { "✓" } else { "✗" };
            let email_short = if diagnosis.email.len() > 18 {
                format!("{}...", &diagnosis.email[..15])
            } else {
                diagnosis.email.clone()
            };

            println!(
                "{:<36} {:<20} {:<20} {:<10}",
                diagnosis.user_id, email_short, status_str, can_decrypt
            );
        }

        println!("\n{}", "=".repeat(90));
        println!("Summary:");
        println!("  Total users:         {}", diagnoses.len());
        println!("  No wallet:           {} ⚠️", no_wallet);
        println!("  Address only:        {} ⚠️", address_only);
        println!("  Valid encryption:    {} ✅", valid);
        println!("  Legacy encryption:   {} ⚠️", legacy);
        println!("  Corrupted:           {} ❌", corrupted);
        println!("\n  Users needing fix:   {}", no_wallet + address_only + legacy + corrupted);

    } else if let Some(email) = args.user {
        // Fix specific user
        println!("🔧 Fixing wallet for user: {}\n", email);

        let results = service.initialize_test_users(&[&email]).await?;
        for result in results {
            if result.success {
                println!("✅ Success: {}", result.action_taken);
                if let Some(addr) = result.new_wallet_address {
                    println!("   Wallet address: {}", addr);
                }
            } else {
                println!("❌ Failed: {}", result.error.unwrap_or_default());
            }
        }

    } else if args.all {
        // Fix all users
        println!("🔧 Fixing all user wallets...\n");

        let report = service.fix_all_users().await?;

        println!("\n{}", "=".repeat(60));
        println!("Wallet Initialization Report");
        println!("{}", "=".repeat(60));
        println!("Total users:              {}", report.total_users);
        println!("Users without wallet:     {}", report.users_without_wallet);
        println!("Users with legacy IV:     {}", report.users_with_legacy_encryption);
        println!("Users with valid crypto:  {}", report.users_with_valid_encryption);
        println!("Users with corrupted:     {}", report.users_with_corrupted_encryption);
        println!("{}", "-".repeat(60));
        println!("Wallets created:          {}", report.wallets_created);
        println!("Wallets re-encrypted:     {}", report.wallets_re_encrypted);
        println!("Duration:                 {:.2}s", report.duration_seconds);

        if !report.errors.is_empty() {
            println!("\n❌ Errors:");
            for error in &report.errors {
                println!("   - {}", error);
            }
        }

    } else {
        println!("No action specified. Use --diagnose, --user <email>, or --all");
        println!("\nExamples:");
        println!("  cargo run --example fix_user_wallets -- --diagnose");
        println!("  cargo run --example fix_user_wallets -- --user test@example.com");
        println!("  cargo run --example fix_user_wallets -- --all");
    }

    Ok(())
}
