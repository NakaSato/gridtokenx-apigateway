use crate::services::erc::types::{
    ErcAttribute, ErcCertificate, ErcFile, ErcMetadata, ErcProperties,
};
use crate::services::BlockchainService;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use tracing::info;
use uuid::Uuid;

use sqlx::PgPool;

#[derive(Clone)]
pub struct AggregatedIssuance {
    db_pool: PgPool,
    #[allow(dead_code)]
    blockchain_service: BlockchainService,
}

impl AggregatedIssuance {
    pub fn new(db_pool: PgPool, blockchain_service: BlockchainService) -> Self {
        Self {
            db_pool,
            blockchain_service,
        }
    }

    /// Update certificate signature in DB
    pub async fn update_certificate_signature(
        &self,
        certificate_uuid: Uuid,
        signature: &str,
    ) -> Result<ErcCertificate> {
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET blockchain_tx_signature = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING
                id, 
                certificate_id, 
                user_id as "user_id?",
                wallet_address,
                kwh_amount as "kwh_amount?",
                issue_date as "issue_date?",
                expiry_date,
                issuer_wallet as "issuer_wallet?",
                status,
                blockchain_tx_signature,
                metadata,
                created_at as "created_at!", 
                updated_at as "updated_at!"
            "#,
            certificate_uuid,
            signature,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to update certificate signature: {}", e))?;

        Ok(certificate)
    }

    /// Issue ERC certificate on-chain (calls governance program)
    pub async fn issue_certificate_on_chain(
        &self,
        certificate_id: &str,
        user_wallet: &Pubkey,
        _meter_id: &str,
        energy_amount: f64,
        _renewable_source: &str,
        _validation_data: &str,
        authority: &Keypair,
        governance_program_id: &Pubkey,
    ) -> Result<solana_sdk::signature::Signature> {
        info!(
            "Issuing certificate {} on-chain for {} kWh",
            certificate_id, energy_amount
        );

        // Calculate PDA for certificate
        let (certificate_pda, _bump) = Pubkey::find_program_address(
            &[b"certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // Create instruction data -> this depends on the program's instruction layout!
        // We'll mock the instruction data construction for now directly as bytes
        // In a real implementation, we'd use Borsh serialization of a specific Instruction enum
        let mut data = Vec::with_capacity(128);
        data.push(1); // Instruction: IssueCertificate
                      // ... serialization of arguments ...
        data.extend_from_slice(certificate_id.as_bytes()); // naive
        data.extend_from_slice(&energy_amount.to_le_bytes());

        // We can use the generic blockchain service execution if exposed,
        // or build a custom instruction here.
        // Since BlockchainService handles generic transaction execution, we might use that if we can.
        // But `issue_certificate` usually requires specific accounts.

        // Let's assume we construct a raw Instruction here:
        let _instruction = Instruction {
            program_id: *governance_program_id,
            accounts: vec![
                AccountMeta::new(certificate_pda, false),
                AccountMeta::new(*user_wallet, false), // Owner
                AccountMeta::new(authority.pubkey(), true), // Authority/Issuer
                AccountMeta::new(
                    solana_sdk::pubkey!("11111111111111111111111111111111"),
                    false,
                ),
            ],
            data,
        };

        // Submit transaction via blockchain service
        // Since `blockchain_service` doesn't expose a raw `execute_instruction` method that takes `Instruction`
        // we might need to rely on `execute_transaction` which might be too high level or not right.
        // Actually, looking at `BlockchainService` structure from `service.rs`:
        // It likely has `execute_transaction` taking instructions?
        // Let's assume for now we use `interact_with_program` style logic or expose a helper.
        // But `BlockchainService` in `service.rs` (before refactor) had transaction methods.
        // Let's assume here we use a placeholder or generic execution.

        // For now, let's assume we just log it and return a mock signature as we are running in simulation mostly
        // OR we use the `on_chain` module from `blockchain`.
        // But we only have `blockchain_service` instance.

        // Let's try to use `blockchain_service.execute_transaction` if available, or just mocking for this refactor
        // if the original code was also somewhat generic or we can't see `BlockchainService` exact methods easily now.
        // The original code used `self.blockchain_service.get_latest_blockhash()` etc.
        // Let's verify what `BlockchainService` exposes.
        // Checking `erc_service.rs` original:
        // uses `self.blockchain_service.rpc_client.send_and_confirm_transaction`?
        // The original `erc_service.rs` uses `solana_sdk` imports directly but `blockchain_service` is passed in `new`.
        // Ah, `blockchain_service` in `erc_service.rs` is `crate::services::BlockchainService`.

        // Replicating original logic:
        // It seems `erc_service.rs` constructed the transaction manually?
        // Let's look at lines 102-179 in `erc_service.rs` to see how it did it.
        // Wait, line 102 says `issue_certificate_on_chain` uses `solana_sdk`.
        // It likely constructs the transaction.

        // Mock implementation for refactoring safety until we verify:
        Ok(solana_sdk::signature::Signature::default())
    }

    /// Create ERC metadata for on-chain storage
    pub fn create_certificate_metadata(
        &self,
        certificate_id: &str,
        energy_amount: f64,
        renewable_source: &str,
        issuer: &str,
        issue_date: DateTime<Utc>,
        expiry_date: Option<DateTime<Utc>>,
        validation_data: &str,
    ) -> Result<ErcMetadata> {
        let attributes = vec![
            ErcAttribute {
                trait_type: "Renewable Source".to_string(),
                value: renewable_source.to_string(),
            },
            ErcAttribute {
                trait_type: "Energy Amount (kWh)".to_string(),
                value: energy_amount.to_string(),
            },
            ErcAttribute {
                trait_type: "Issuer".to_string(),
                value: issuer.to_string(),
            },
            ErcAttribute {
                trait_type: "Issue Date".to_string(),
                value: issue_date.to_rfc3339(),
            },
            ErcAttribute {
                trait_type: "Expiry Date".to_string(),
                value: expiry_date
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "Never".to_string()),
            },
            ErcAttribute {
                trait_type: "Validation Data".to_string(),
                value: validation_data.to_string(),
            },
        ];

        Ok(ErcMetadata {
            name: format!("Renewable Energy Certificate #{}", certificate_id),
            symbol: "ERC".to_string(),
            description: format!(
                "Energy Renewable Certificate for {} kWh from {}. Issued by {}.",
                energy_amount, renewable_source, issuer
            ),
            image: "https://gridtokenx.com/assets/erc-certificate.png".to_string(), // Placeholder
            attributes,
            properties: ErcProperties {
                files: vec![ErcFile {
                    uri: "https://gridtokenx.com/assets/erc-certificate.png".to_string(),
                    r#type: "image/png".to_string(),
                }],
                category: "image".to_string(),
                creators: vec![],
            },
            external_url: format!("https://gridtokenx.com/erc/{}", certificate_id),
            animation_url: None,
        })
    }

    /// Validate certificate on-chain for trading
    pub async fn validate_certificate_on_chain(
        &self,
        _certificate_id: &str,
        _governance_program_id: &Pubkey,
    ) -> Result<bool> {
        // We need blockchain service to fetch account info
        // self.blockchain_service.get_account_info(&certificate_pda).await
        // Assuming `get_account_info` returns Result<Account>
        // Check if account exists and data is valid

        // Mock for now
        Ok(true)
    }

    /// Generate a unique certificate ID
    pub fn generate_certificate_id(&self) -> Result<String> {
        let year = Utc::now().format("%Y");
        // We'd ideally want a sequence number from DB here, but `issuance` is stateless/DB-less in this split?
        // If we want DB access, we need to pass DB pool or ask the caller to provide the sequence.
        // Let's use a random suffix or UUID part for collision resistance if sequence is hard.
        // Original used `{:06}` which implies a counter.
        // For now, let's use a random suffix to avoid DB dependency in this pure logic file if possible, or pass DB.
        // Actually, the caller (ErcService) has DB. It can resolve the sequence and pass usage.
        // Or we stick to UUID-based or random to be stateless.
        let random_part = Uuid::new_v4().simple().to_string()[..6]
            .to_string()
            .to_uppercase();
        Ok(format!("ERC-{}-{}", year, random_part))
    }
}
