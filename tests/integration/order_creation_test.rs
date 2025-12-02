use anyhow::Result;
use api_gateway::services::blockchain_service::BlockchainService;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signer::{Signer, keypair::Keypair},
};
use std::str::FromStr;
use std::sync::Arc;

#[tokio::test]
async fn test_build_create_order_instruction() -> Result<()> {
    // Setup
    let rpc_url = "http://127.0.0.1:8899".to_string();
    let cluster = "localnet".to_string();
    let blockchain_service = BlockchainService::new(rpc_url, cluster)?;

    // Test parameters
    let market_pubkey = "GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk";
    let energy_amount = 100_000; // 100 kWh * 1000
    let price_per_kwh = 150_000; // 0.15 USD * 1,000,000
    let order_type = "sell";
    let erc_certificate_id = Some("CERT-12345");

    // Build instruction
    // Build instruction
    let instruction = blockchain_service
        .build_create_order_instruction(
            market_pubkey,
            energy_amount,
            price_per_kwh,
            order_type,
            erc_certificate_id,
        )
        .await?;

    // Verify instruction
    println!("Instruction Program ID: {}", instruction.program_id);
    assert_eq!(
        instruction.program_id,
        Pubkey::from_str("GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk")?
    );

    // Verify accounts
    // 1. Market
    // 2. Order (PDA)
    // 3. Payer
    // 4. System Program
    // 5. ERC Certificate (optional)
    assert_eq!(instruction.accounts.len(), 5);

    // Market
    assert_eq!(
        instruction.accounts[0].pubkey,
        Pubkey::from_str(market_pubkey)?
    );
    assert!(!instruction.accounts[0].is_signer);
    assert!(instruction.accounts[0].is_writable);

    // Order (PDA)
    // Should be writable and not a signer (init_if_needed or init usually requires signature if it's a keypair, but for PDA it's just seeds)
    // Wait, for `init` with PDA, the PDA itself is not a signer. The payer is the signer.
    assert!(instruction.accounts[1].is_writable);
    assert!(!instruction.accounts[1].is_signer);

    Ok(())
}
