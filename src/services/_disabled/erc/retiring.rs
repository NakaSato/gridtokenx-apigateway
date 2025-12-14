use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

use crate::services::erc::types::ErcCertificate;

use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use crate::services::BlockchainService;

/// Manager for retiring ERC certificates
#[derive(Clone)]
pub struct CertificateRetiring {
    db_pool: PgPool,
    blockchain_service: BlockchainService,
}

impl CertificateRetiring {
    pub fn new(db_pool: PgPool, blockchain_service: BlockchainService) -> Self {
        Self {
            db_pool,
            blockchain_service,
        }
    }

    /// Retire certificate on-chain (Revoke)
    pub async fn retire_certificate_on_chain(
        &self,
        certificate_id: &str,
        authority: &Keypair,
        governance_program_id: &Pubkey,
    ) -> Result<String> {
        let (poa_config, _) = Pubkey::find_program_address(&[b"poa_config"], governance_program_id);

        let (erc_certificate, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // Discriminator for "global:revoke_erc"
        let discriminator: [u8; 8] = [0x10, 0x30, 0x71, 0x55, 0x76, 0x46, 0xb9, 0x96];

        // Let's implement the structure first.
        let instruction_data = {
            let mut data = Vec::new();
            data.extend_from_slice(&discriminator);
            // reason: String
            let reason = "Retired via API Gateway";
            data.extend_from_slice(&(reason.len() as u32).to_le_bytes());
            data.extend_from_slice(reason.as_bytes());
            data
        };

        // Accounts: poa_config, erc_certificate, authority
        let accounts = vec![
            AccountMeta::new(poa_config, false),
            AccountMeta::new(erc_certificate, false),
            AccountMeta::new(authority.pubkey(), true),
        ];

        let instruction = Instruction {
            program_id: *governance_program_id,
            accounts,
            data: instruction_data,
        };

        let recent_blockhash = self.blockchain_service.get_latest_blockhash().await?;
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&authority.pubkey()),
            &[authority],
            recent_blockhash,
        );

        let signature = self
            .blockchain_service
            .send_and_confirm_transaction(&transaction)
            .await?;

        Ok(signature.to_string())
    }

    /// Retire a certificate
    pub async fn retire_certificate(&self, certificate_uuid: Uuid) -> Result<ErcCertificate> {
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET status = 'Retired'
            WHERE id = $1 AND status = 'Active'
            RETURNING
                id, certificate_id,
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
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to retire certificate: {}", e))?
        .ok_or_else(|| anyhow!("Certificate not found or already retired"))?;

        info!("Certificate {} retired", certificate.certificate_id);

        Ok(certificate)
    }
}
