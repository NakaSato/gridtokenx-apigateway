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
#[derive(Clone, Debug)]
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

        // Resolve new user_id if wallet exists in database
        let new_user = sqlx::query!("SELECT id FROM users WHERE wallet_address = $1", to_wallet)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| anyhow!("Failed to resolve new user: {}", e))?;
        
        let new_user_id = new_user.map(|r| r.id);
        
        let to_user_id = new_user_id.ok_or_else(|| anyhow!("Recipient user not found for wallet: {}", to_wallet))?;

        // Get current owner (from_user_id)
        let current_cert = sqlx::query!("SELECT user_id FROM erc_certificates WHERE id = $1", certificate_uuid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| anyhow!("Failed to fetch certificate: {}", e))?
            .ok_or_else(|| anyhow!("Certificate not found"))?;
            
        let from_user_id = current_cert.user_id;

        // Update certificate wallet and status
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET wallet_address = $2, status = 'transferred', user_id = $3
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
                settlement_id,
                created_at as "created_at!",
                updated_at as "updated_at!"
            "#,
            certificate_uuid,
            to_wallet,
            new_user_id,
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
                transfer_date, blockchain_tx_signature,
                from_user_id, to_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
            from_user_id,
            to_user_id,
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
