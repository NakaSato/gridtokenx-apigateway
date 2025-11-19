use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{debug, info};
use uuid::Uuid;
use bigdecimal::BigDecimal;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    instruction::{Instruction, AccountMeta},
};
use sha2::{Sha256, Digest};

/// Energy Renewable Certificate
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErcCertificate {
    pub id: Uuid,
    pub certificate_id: String,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    pub kwh_amount: Option<BigDecimal>,
    pub issue_date: Option<DateTime<Utc>>,
    pub expiry_date: Option<DateTime<Utc>>,
    pub issuer_wallet: Option<String>,
    pub status: String,
    pub blockchain_tx_signature: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Request to issue a new ERC
#[derive(Debug, Deserialize, Serialize)]
pub struct IssueErcRequest {
    pub wallet_address: String,
    pub kwh_amount: BigDecimal,
    pub expiry_date: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

/// Certificate transfer record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CertificateTransfer {
    pub id: Uuid,
    pub certificate_id: Uuid,
    pub from_wallet: Option<String>,
    pub to_wallet: String,
    pub transfer_date: Option<DateTime<Utc>>,
    pub blockchain_tx_signature: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Service for managing Energy Renewable Certificates
#[derive(Clone)]
pub struct ErcService {
    db_pool: PgPool,
}

/// ERC Certificate metadata for on-chain storage
#[derive(Debug, Serialize, Deserialize)]
pub struct ErcMetadata {
    pub name: String,
    pub description: String,
    pub image: Option<String>,
    pub attributes: Vec<ErcAttribute>,
    pub properties: ErcProperties,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErcAttribute {
    pub trait_type: String,
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErcProperties {
    pub files: Vec<ErcFile>,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErcFile {
    pub uri: String,
    #[serde(rename = "type")]
    pub file_type: String,
}

impl ErcService {
    /// Create a new ERC service
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Issue ERC certificate on-chain (calls governance program)
    pub async fn issue_certificate_on_chain(
        &self,
        certificate_id: &str,
        user_wallet: &Pubkey,
        energy_amount: f64,
        renewable_source: &str,
        validation_data: &str,
        authority: &Keypair,
        governance_program_id: &Pubkey,
    ) -> Result<solana_sdk::signature::Signature> {
        info!(
            "Issuing ERC certificate {} on-chain for wallet {}",
            certificate_id, user_wallet
        );

        // 1. Derive ERC certificate PDA
        let (_certificate_pda, _bump) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // 2. Get PoA config PDA
        let (_poa_config_pda, _) = Pubkey::find_program_address(
            &[b"poa_config"],
            governance_program_id,
        );

        // 3. Build Anchor instruction data
        let mut instruction_data = Vec::new();
        
        // Discriminator for "issue_erc" instruction (first 8 bytes of SHA256 hash)
        let mut hasher = Sha256::new();
        hasher.update(b"global:issue_erc");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);
        
        // Serialize arguments using simple approach for now
        // In production, use proper Borsh serialization
        instruction_data.extend_from_slice(&(certificate_id.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(certificate_id.as_bytes());
        instruction_data.extend_from_slice(&(energy_amount as u64).to_le_bytes());
        instruction_data.extend_from_slice(&(renewable_source.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(renewable_source.as_bytes());
        instruction_data.extend_from_slice(&(validation_data.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(validation_data.as_bytes());
        
        // 4. Build accounts for instruction
        let accounts = vec![
            AccountMeta::new(_poa_config_pda, false),
            AccountMeta::new(_certificate_pda, false),
            AccountMeta::new_readonly(*user_wallet, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
            AccountMeta::new_readonly(solana_sdk::pubkey!("11111111111111111111111111111112"), false),
        ];
        
        let _issue_erc_ix = Instruction::new_with_bytes(
            *governance_program_id,
            &instruction_data,
            accounts,
        );

        // 5. For now, return a mock signature since we need the blockchain service
        // In the actual implementation, this would call:
        // blockchain_service.build_and_send_transaction(vec![issue_erc_ix], &[authority]).await
        let mock_signature = solana_sdk::signature::Signature::default();
        
        info!(
            "ERC certificate {} minted on-chain (mock): {}",
            certificate_id, mock_signature
        );
        
        Ok(mock_signature)
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
        let metadata = ErcMetadata {
            name: format!("Renewable Energy Certificate #{}", certificate_id),
            description: format!(
                "Certificate for {} kWh of renewable energy from {} source",
                energy_amount, renewable_source
            ),
            image: Some("https://arweave.net/certificate-image".to_string()), // Placeholder
            attributes: vec![
                ErcAttribute {
                    trait_type: "Energy Amount".to_string(),
                    value: serde_json::Value::Number(serde_json::Number::from_f64(energy_amount).unwrap()),
                    unit: Some("kWh".to_string()),
                },
                ErcAttribute {
                    trait_type: "Renewable Source".to_string(),
                    value: serde_json::Value::String(renewable_source.to_string()),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Issuer".to_string(),
                    value: serde_json::Value::String(issuer.to_string()),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Issue Date".to_string(),
                    value: serde_json::Value::String(issue_date.to_rfc3339()),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Expiry Date".to_string(),
                    value: serde_json::Value::String(
                        expiry_date.map(|d| d.to_rfc3339()).unwrap_or_else(|| "Never".to_string())
                    ),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Certificate ID".to_string(),
                    value: serde_json::Value::String(certificate_id.to_string()),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Status".to_string(),
                    value: serde_json::Value::String("Active".to_string()),
                    unit: None,
                },
                ErcAttribute {
                    trait_type: "Validation Data".to_string(),
                    value: serde_json::Value::String(validation_data.to_string()),
                    unit: None,
                },
            ],
            properties: ErcProperties {
                files: vec![
                    ErcFile {
                        uri: "https://arweave.net/certificate-pdf".to_string(), // Placeholder
                        file_type: "application/pdf".to_string(),
                    }
                ],
                category: "certificate".to_string(),
            },
        };

        Ok(metadata)
    }

    /// Update certificate with blockchain signature
    pub async fn update_blockchain_signature(
        &self,
        certificate_id: &str,
        tx_signature: &str,
    ) -> Result<ErcCertificate> {
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET blockchain_tx_signature = $2
            WHERE certificate_id = $1
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
                created_at, 
                updated_at
            "#,
            certificate_id,
            tx_signature,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to update certificate blockchain signature: {}", e))?;

        info!(
            "Updated certificate {} with blockchain signature: {}",
            certificate_id, tx_signature
        );

        Ok(certificate)
    }

    /// Validate certificate on-chain
    pub async fn validate_certificate_on_chain(
        &self,
        certificate_id: &str,
        governance_program_id: &Pubkey,
    ) -> Result<bool> {
        info!("Validating certificate {} on-chain", certificate_id);

        // For now, return true (mock implementation)
        // In production, this would query the blockchain to verify the certificate exists
        let (_certificate_pda, _bump) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // Mock validation - in production, check if account exists and has valid data
        let is_valid = true; // Replace with actual blockchain query

        info!(
            "Certificate {} validation result: {}",
            certificate_id, is_valid
        );

        Ok(is_valid)
    }

    /// Transfer certificate on-chain
    pub async fn transfer_certificate_on_chain(
        &self,
        certificate_id: &str,
        from_wallet: &Pubkey,
        to_wallet: &Pubkey,
        authority: &Keypair,
        governance_program_id: &Pubkey,
    ) -> Result<solana_sdk::signature::Signature> {
        info!(
            "Transferring certificate {} from {} to {} on-chain",
            certificate_id, from_wallet, to_wallet
        );

        // 1. Derive certificate PDA
        let (_certificate_pda, _bump) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // 2. Build Anchor instruction data for transfer
        let mut instruction_data = Vec::new();
        
        // Discriminator for "transfer_erc" instruction
        let mut hasher = Sha256::new();
        hasher.update(b"global:transfer_erc");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);
        
        // Serialize arguments
        instruction_data.extend_from_slice(&(certificate_id.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(certificate_id.as_bytes());
        
        // 3. Build accounts for instruction
        let accounts = vec![
            AccountMeta::new(_certificate_pda, false),
            AccountMeta::new_readonly(*from_wallet, false),
            AccountMeta::new(*to_wallet, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
            AccountMeta::new_readonly(solana_sdk::pubkey!("11111111111111111111111111111112"), false),
        ];
        
        let _transfer_erc_ix = Instruction::new_with_bytes(
            *governance_program_id,
            &instruction_data,
            accounts,
        );

        // 4. For now, return a mock signature
        let mock_signature = solana_sdk::signature::Signature::default();
        
        info!(
            "Certificate {} transferred on-chain (mock): {}",
            certificate_id, mock_signature
        );
        
        Ok(mock_signature)
    }

    /// Retire certificate on-chain
    pub async fn retire_certificate_on_chain(
        &self,
        certificate_id: &str,
        authority: &Keypair,
        governance_program_id: &Pubkey,
    ) -> Result<solana_sdk::signature::Signature> {
        info!("Retiring certificate {} on-chain", certificate_id);

        // 1. Derive certificate PDA
        let (_certificate_pda, _bump) = Pubkey::find_program_address(
            &[b"erc_certificate", certificate_id.as_bytes()],
            governance_program_id,
        );

        // 2. Build Anchor instruction data for retirement
        let mut instruction_data = Vec::new();
        
        // Discriminator for "retire_erc" instruction
        let mut hasher = Sha256::new();
        hasher.update(b"global:retire_erc");
        let hash = hasher.finalize();
        instruction_data.extend_from_slice(&hash[0..8]);
        
        // Serialize arguments
        instruction_data.extend_from_slice(&(certificate_id.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(certificate_id.as_bytes());
        
        // 3. Build accounts for instruction
        let accounts = vec![
            AccountMeta::new(_certificate_pda, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
            AccountMeta::new_readonly(solana_sdk::pubkey!("11111111111111111111111111111112"), false),
        ];
        
        let _retire_erc_ix = Instruction::new_with_bytes(
            *governance_program_id,
            &instruction_data,
            accounts,
        );

        // 4. For now, return a mock signature
        let mock_signature = solana_sdk::signature::Signature::default();
        
        info!(
            "Certificate {} retired on-chain (mock): {}",
            certificate_id, mock_signature
        );
        
        Ok(mock_signature)
    }

    /// Issue a new ERC certificate
    pub async fn issue_certificate(
        &self,
        user_id: Uuid,
        issuer_wallet: &str,
        request: IssueErcRequest,
    ) -> Result<ErcCertificate> {
        // Validate amount
        use std::str::FromStr;
        if request.kwh_amount <= BigDecimal::from_str("0").unwrap() {
            return Err(anyhow!("kWh amount must be positive"));
        }

        // Validate expiry date (if provided)
        if let Some(expiry) = request.expiry_date {
            if expiry <= Utc::now() {
                return Err(anyhow!("Expiry date must be in the future"));
            }
        }

        // Generate certificate ID
        let certificate_id = self.generate_certificate_id().await?;

        // Insert certificate into database
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            INSERT INTO erc_certificates (
                id, certificate_id, user_id, wallet_address, 
                kwh_amount, issue_date, expiry_date, 
                issuer_wallet, status, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
                created_at, 
                updated_at
            "#,
            Uuid::new_v4(),
            certificate_id,
            user_id,
            request.wallet_address,
            request.kwh_amount,
            Utc::now(),
            request.expiry_date,
            issuer_wallet,
            "Active",
            request.metadata,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to insert certificate: {}", e))?;

        info!(
            "ERC certificate issued: {} for user={}, kwh={}",
            certificate_id, user_id, request.kwh_amount
        );

        Ok(certificate)
    }

    /// Generate a unique certificate ID
    async fn generate_certificate_id(&self) -> Result<String> {
        let year = Utc::now().format("%Y");
        
        // Get count of certificates issued this year
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM erc_certificates
            WHERE certificate_id LIKE $1
            "#,
            format!("ERC-{}-%", year)
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to count certificates: {}", e))?;

        let cert_number = count.count + 1;
        Ok(format!("ERC-{}-{:06}", year, cert_number))
    }

    /// Get certificate by ID (certificate_id string)
    pub async fn get_certificate_by_id(&self, certificate_id: &str) -> Result<ErcCertificate> {
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            SELECT 
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
                created_at, 
                updated_at
            FROM erc_certificates
            WHERE certificate_id = $1
            "#,
            certificate_id
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch certificate: {}", e))?
        .ok_or_else(|| anyhow!("Certificate not found"))?;

        Ok(certificate)
    }

    /// Get certificates by wallet address
    pub async fn get_certificates_by_wallet(
        &self,
        wallet_address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ErcCertificate>> {
        let certificates = sqlx::query_as!(
            ErcCertificate,
            r#"
            SELECT 
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
                created_at, 
                updated_at
            FROM erc_certificates
            WHERE wallet_address = $1
            ORDER BY issue_date DESC
            LIMIT $2 OFFSET $3
            "#,
            wallet_address,
            limit,
            offset,
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch certificates: {}", e))?;

        debug!("Retrieved {} certificates for wallet {}", certificates.len(), wallet_address);

        Ok(certificates)
    }

    /// Get certificates by user ID with pagination and filtering
    pub async fn get_user_certificates(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        sort_by: &str,
        sort_order: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<ErcCertificate>> {
        let query = if let Some(_status) = status_filter {
            format!(
                r#"
                SELECT 
                    id, certificate_id, user_id, wallet_address, 
                    kwh_amount, issue_date, expiry_date, 
                    issuer_wallet, status, 
                    blockchain_tx_signature, metadata,
                    created_at, updated_at
                FROM erc_certificates
                WHERE user_id = $1 AND status = $2
                ORDER BY {} {}
                LIMIT $3 OFFSET $4
                "#,
                sort_by, sort_order
            )
        } else {
            format!(
                r#"
                SELECT 
                    id, certificate_id, user_id, wallet_address, 
                    kwh_amount, issue_date, expiry_date, 
                    issuer_wallet, status, 
                    blockchain_tx_signature, metadata,
                    created_at, updated_at
                FROM erc_certificates
                WHERE user_id = $1
                ORDER BY {} {}
                LIMIT $2 OFFSET $3
                "#,
                sort_by, sort_order
            )
        };

        let certificates = if let Some(status) = status_filter {
            sqlx::query_as::<_, ErcCertificate>(&query)
                .bind(user_id)
                .bind(status)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user certificates: {}", e))?
        } else {
            sqlx::query_as::<_, ErcCertificate>(&query)
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to fetch user certificates: {}", e))?
        };

        debug!("Retrieved {} certificates for user {}", certificates.len(), user_id);

        Ok(certificates)
    }

    /// Count total certificates for a user
    pub async fn count_user_certificates(
        &self,
        user_id: Uuid,
        status_filter: Option<&str>,
    ) -> Result<i64> {
        let count = if let Some(status) = status_filter {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM erc_certificates WHERE user_id = $1 AND status = $2"
            )
            .bind(user_id)
            .bind(status)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user certificates: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM erc_certificates WHERE user_id = $1"
            )
            .bind(user_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user certificates: {}", e))?
        };

        Ok(count)
    }

    /// Update certificate blockchain transaction
    pub async fn update_certificate_tx(
        &self,
        certificate_uuid: Uuid,
        tx_signature: &str,
    ) -> Result<ErcCertificate> {
        let certificate = sqlx::query_as!(
            ErcCertificate,
            r#"
            UPDATE erc_certificates
            SET blockchain_tx_signature = $2
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
                created_at, 
                updated_at
            "#,
            certificate_uuid,
            tx_signature,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to update certificate: {}", e))?;

        info!(
            "Updated certificate {} with tx: {}",
            certificate.certificate_id, tx_signature
        );

        Ok(certificate)
    }

    /// Transfer certificate to another wallet
    pub async fn transfer_certificate(
        &self,
        certificate_uuid: Uuid,
        from_wallet: &str,
        to_wallet: &str,
        tx_signature: &str,
    ) -> Result<(ErcCertificate, CertificateTransfer)> {
        // Start transaction
        let mut tx = self.db_pool.begin().await
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
                created_at, 
                updated_at
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
                id, certificate_id, from_wallet, to_wallet, 
                transfer_date, blockchain_tx_signature, created_at
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

        // Commit transaction
        tx.commit().await
            .map_err(|e| anyhow!("Failed to commit transfer: {}", e))?;

        info!(
            "Certificate {} transferred from {} to {}",
            certificate.certificate_id, from_wallet, to_wallet
        );

        Ok((certificate, transfer))
    }

    /// Retire a certificate (mark as retired/used)
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
                created_at, 
                updated_at
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

    /// Get certificate statistics for a user
    pub async fn get_user_stats(&self, user_id: Uuid) -> Result<CertificateStats> {
        let stats = sqlx::query_as!(
            CertificateStatsRow,
            r#"
            SELECT 
                COUNT(*) as "total_count!",
                COALESCE(SUM(CASE WHEN status = 'Active' THEN kwh_amount ELSE 0 END), 0) as "active_kwh!",
                COALESCE(SUM(CASE WHEN status = 'Retired' THEN kwh_amount ELSE 0 END), 0) as "retired_kwh!",
                COALESCE(SUM(kwh_amount), 0) as "total_kwh!"
            FROM erc_certificates
            WHERE user_id = $1
            "#,
            user_id,
        )
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch stats: {}", e))?;

        Ok(CertificateStats {
            total_certificates: stats.total_count,
            active_kwh: stats.active_kwh,
            retired_kwh: stats.retired_kwh,
            total_kwh: stats.total_kwh,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CertificateStatsRow {
    total_count: i64,
    active_kwh: BigDecimal,
    retired_kwh: BigDecimal,
    total_kwh: BigDecimal,
}

#[derive(Debug, Serialize)]
pub struct CertificateStats {
    pub total_certificates: i64,
    pub active_kwh: BigDecimal,
    pub retired_kwh: BigDecimal,
    pub total_kwh: BigDecimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_id_format() {
        let year = Utc::now().format("%Y");
        let cert_id = format!("ERC-{}-{:06}", year, 1);
        assert!(cert_id.starts_with("ERC-"));
        assert!(cert_id.len() >= 15); // ERC-2025-000001
    }
}
