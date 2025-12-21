use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::erc::types::{CertificateTransfer, ErcCertificate};
use crate::services::BlockchainService;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
};

/// Manager for transferring ERC certificates
#[derive(Clone)]
pub struct CertificateTransferManager {
    db_pool: PgPool,
    blockchain_service: BlockchainService,
}

impl CertificateTransferManager {
    pub fn new(db_pool: PgPool, blockchain_service: BlockchainService) -> Self {
        Self {
            db_pool,
            blockchain_service,
        }
    }

    /// Transfer certificate on-chain
    pub async fn transfer_certificate_on_chain(
        &self,
        certificate_id: &str,
        owner: &Keypair, // Owner keypair
        to_owner_pubkey: &Pubkey,
        _governance_program_id: &Pubkey,
    ) -> Result<String> {
        let signature = self
            .blockchain_service
            .transfer_erc(certificate_id, owner, to_owner_pubkey)
            .await?;

        Ok(signature.to_string())
    }

    /// Transfer a certificate to another wallet
    pub async fn transfer_certificate(
        &self,
        certificate_uuid: Uuid,
        from_wallet: &str,
        to_wallet: &str,
        tx_signature: &str,
    ) -> Result<(ErcCertificate, CertificateTransfer)> {
        let mut tx = self
            .db_pool
            .begin()
            .await
            .map_err(|e| anyhow!("Failed to start transaction: {}", e))?;

        // Update certificate wallet and status
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET wallet_address = $2, status = 'Transferred'
            WHERE id = $1
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
            to_wallet,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to update certificate: {}", e))?;

        // Record transfer
        let transfer = sqlx::query_as!(
            CertificateTransfer,
            r#"
            INSERT INTO erc_certificate_transfers (
                id, certificate_id, from_wallet, to_wallet,
                transfer_date, blockchain_tx_signature
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id as "id!",
                certificate_id as "certificate_id!",
                from_wallet as "from_wallet!",
                to_wallet as "to_wallet!",
                transfer_date as "transfer_date!",
                blockchain_tx_signature as "blockchain_tx_signature!",
                created_at as "created_at!"
            "#,
            Uuid::new_v4(),
            certificate_uuid,
            from_wallet,
            to_wallet,
            Utc::now(),
            tx_signature,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to record transfer: {}", e))?;

        tx.commit()
            .await
            .map_err(|e| anyhow!("Failed to commit transfer: {}", e))?;

        Ok((certificate, transfer))
    }
}
