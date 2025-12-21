use crate::services::erc::types::{
    ErcAttribute, ErcCertificate, ErcFile, ErcMetadata, ErcProperties,
};
use crate::services::BlockchainService;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
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
        meter_id: &str,
        energy_amount: f64,
        renewable_source: &str,
        validation_data: &str,
        authority: &Keypair,
        _governance_program_id: &Pubkey,
    ) -> Result<solana_sdk::signature::Signature> {
        info!(
            "Issuing certificate {} on-chain for {} kWh",
            certificate_id, energy_amount
        );

        // Map energy amount to u64 (lamports-like precision)
        let amount_u64 = (energy_amount * 1000.0) as u64;

        // Get meter PDA from registry
        let registry_program_id = self.blockchain_service.registry_program_id()?;
        let (meter_pda, _) = Pubkey::find_program_address(
            &[b"meter", meter_id.as_bytes()],
            &registry_program_id,
        );

        // Submit real transaction via blockchain service
        self.blockchain_service.issue_erc(
            certificate_id,
            user_wallet,
            &meter_pda,
            amount_u64,
            renewable_source,
            validation_data,
            authority,
        ).await
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
