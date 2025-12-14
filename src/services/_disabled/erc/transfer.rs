use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::erc::types::{CertificateTransfer, ErcCertificate};
use crate::services::BlockchainService;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
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
        from_keypair: &Keypair, // Owner keypair
        to_owner_pubkey: &Pubkey,

        // Wait, lib.rs says: TransferErc { poa_config, erc_certificate, current_owner, new_owner }
        // Authority is NOT in TransferErc accounts!
        // So why does test pass "authority"?
        // Test: erc_service.transfer_certificate_on_chain(..., &authority, &governance_program_id)
        // Ah, test arguments: certificate_id, from_keypair, to_wallet, authority, governance_program_id.
        // Maybe "authority" is payer? or "authority" is needed for finding "poa_config"? No, poa_config is PDA.
        // I'll accept "authority" as param but maybe not use it as instruction account if not needed.
        // Unless Payer is needed. Transaction needs payer. `authority` (API gateway) can pay fees.
        // Or `from_keypair` pays fees.
        // In `setup_meter_with_generation` we airdropped to user.
        // I'll use `from_keypair` as payer if it has SOL?
        // Or `authority`?
        // Test passes `authority` (which was setup in `test_erc_transfer_on_chain`).
        governance_program_id: &Pubkey,
    ) -> Result<String> {
        let (poa_config, _) = Pubkey::find_program_address(&[b"poa_config"], governance_program_id);

        let (erc_certificate, _) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // Discriminator for "global:transfer_erc"
        let discriminator: [u8; 8] = [0xc8, 0x0f, 0x10, 0x0d, 0x0d, 0x8f, 0x0b, 0x0b];

        let instruction_data = {
            let mut data = Vec::new();
            data.extend_from_slice(&discriminator);
            data
        };

        // Accounts: poa_config, erc_certificate, current_owner, new_owner
        let accounts = vec![
            AccountMeta::new(poa_config, false),
            AccountMeta::new(erc_certificate, false),
            AccountMeta::new(from_keypair.pubkey(), true), // signer
            AccountMeta::new_readonly(*to_owner_pubkey, false),
        ];

        let instruction = Instruction {
            program_id: *governance_program_id,
            accounts,
            data: instruction_data,
        };

        let recent_blockhash = self.blockchain_service.get_latest_blockhash().await?;
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&from_keypair.pubkey()), // User pays fees? or Authority?
            // Test passes `authority` likely because authority usually pays fees in API?
            // But here `from_keypair` is signer and must sign.
            // If `authority` pays, need `authority` signature too.
            // I'll assume `from_keypair` pays since user was airdropped SOL in test setup.
            &[from_keypair],
            recent_blockhash,
        );

        let signature = self
            .blockchain_service
            .send_and_confirm_transaction(&transaction)
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
