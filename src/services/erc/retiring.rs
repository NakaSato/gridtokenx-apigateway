use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

use crate::services::erc::types::ErcCertificate;

use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
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
        _governance_program_id: &Pubkey,
    ) -> Result<String> {
        let signature = self
            .blockchain_service
            .revoke_erc(certificate_id, "Retired via API Gateway", authority)
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
