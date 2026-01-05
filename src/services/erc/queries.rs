use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use crate::services::erc::types::{CertificateStats, ErcCertificate};
use crate::services::BlockchainService;

/// Manager for Energy Renewable Certificate queries
#[derive(Clone, Debug)]
pub struct ErcQueryManager {
    db_pool: PgPool,
    #[allow(dead_code)]
    blockchain_service: BlockchainService,
}

impl ErcQueryManager {
    /// Create a new ERC query manager
    pub fn new(db_pool: PgPool, blockchain_service: BlockchainService) -> Self {
        Self {
            db_pool,
            blockchain_service,
        }
    }

    #[instrument(skip(self))]
    pub async fn get_user_stats(&self, user_id: Uuid) -> Result<CertificateStats> {
        let total_certificates = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM erc_certificates 
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await?
        .count
        .unwrap_or(0);

        let _active_certificates = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM erc_certificates 
            WHERE user_id = $1 AND status = 'Active'
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await?
        .count
        .unwrap_or(0);

        let _retired_certificates = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM erc_certificates 
            WHERE user_id = $1 AND status = 'Retired'
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await?
        .count
        .unwrap_or(0);

        let total_energy = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(kwh_amount), 0) as total
            FROM erc_certificates 
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.db_pool)
        .await?
        .total
        .unwrap_or(rust_decimal::Decimal::ZERO);

        Ok(CertificateStats {
            total_certificates,
            active_kwh: rust_decimal::Decimal::ZERO, // Need to fetch active kwh?
            retired_kwh: rust_decimal::Decimal::ZERO, // Need to fetch retired kwh?
            total_kwh: total_energy,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_certificate_by_id(&self, certificate_id: &str) -> Result<ErcCertificate> {
        let cert = sqlx::query_as!(
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
                settlement_id,
                created_at as "created_at!",
                updated_at as "updated_at!"
            FROM erc_certificates
            WHERE certificate_id = $1
            "#,
            certificate_id
        )
        .fetch_optional(&self.db_pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Certificate not found"))?;

        Ok(cert)
    }

    #[instrument(skip(self))]
    pub async fn get_my_certificates(&self, user_id: Uuid) -> Result<Vec<ErcCertificate>> {
        let certificates = sqlx::query_as!(
            ErcCertificate,
            r#"
            SELECT
                id, certificate_id, user_id, wallet_address,
                kwh_amount, issue_date, expiry_date,
                issuer_wallet, status,
                blockchain_tx_signature, metadata, settlement_id,
                created_at as "created_at!",
                updated_at as "updated_at!"
            FROM erc_certificates
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch user certificates: {}", e))?;

        Ok(certificates)
    }

    /// Get certificates by user ID with pagination and filtering
    #[instrument(skip(self))]
    pub async fn get_user_certificates(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        sort_by: &str,
        sort_order: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<ErcCertificate>> {
        // Need to construct dynamic query safely or use conditional
        let query = if let Some(_status) = status_filter {
            format!(
                r#"
                SELECT
                    id, certificate_id, user_id, wallet_address,
                    kwh_amount, issue_date, expiry_date,
                    issuer_wallet, status,
                    blockchain_tx_signature, metadata, settlement_id,
                    created_at as "created_at!",
                updated_at as "updated_at!"
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
                    blockchain_tx_signature, metadata, settlement_id,
                    created_at as "created_at!",
                updated_at as "updated_at!"
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

        Ok(certificates)
    }

    /// Count total certificates for a user
    #[instrument(skip(self))]
    pub async fn count_user_certificates(
        &self,
        user_id: Uuid,
        status_filter: Option<&str>,
    ) -> Result<i64> {
        let count = if let Some(status) = status_filter {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM erc_certificates WHERE user_id = $1 AND status = $2",
            )
            .bind(user_id)
            .bind(status)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| anyhow!("Failed to count user certificates: {}", e))?
        } else {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM erc_certificates WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.db_pool)
                .await
                .map_err(|e| anyhow!("Failed to count user certificates: {}", e))?
        };

        Ok(count)
    }

    #[instrument(skip(self))]
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
                settlement_id,
                created_at as "created_at!",
                updated_at as "updated_at!"
            FROM erc_certificates
            WHERE wallet_address = $1
            ORDER BY issue_date DESC
            LIMIT $2 OFFSET $3
            "#,
            wallet_address,
            limit,
            offset
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch certificates: {}", e))?;

        Ok(certificates)
    }
}
