// Transaction Coordinator Service
// Provides unified transaction tracking by routing to existing services

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::transaction::{
    BlockchainOperation, TransactionFilters, TransactionMonitoringConfig, TransactionResponse,
    TransactionRetryRequest, TransactionRetryResponse, TransactionStats,
};
use crate::services::settlement::SettlementService;
use crate::services::transaction::monitoring::TransactionMonitorService;
use crate::services::transaction::query::TransactionQueryService;
use crate::services::transaction::recovery::TransactionRecoveryService;
use crate::services::validation::TransactionValidationService;
use crate::services::BlockchainService;

/// Transaction Coordinator for unified tracking and monitoring
#[derive(Clone)]
pub struct TransactionCoordinator {
    query_service: TransactionQueryService,
    monitor_service: TransactionMonitorService,
    recovery_service: TransactionRecoveryService,
    #[allow(dead_code)]
    config: TransactionMonitoringConfig, // Kept for access if needed, though services have their own
}

impl TransactionCoordinator {
    /// Create a new transaction coordinator
    pub fn new(
        db: PgPool,
        blockchain_service: Arc<BlockchainService>,
        settlement: Arc<SettlementService>,
        _validation_service: Arc<TransactionValidationService>,
    ) -> Self {
        Self::with_config(
            db,
            blockchain_service,
            settlement,
            _validation_service,
            TransactionMonitoringConfig::default(),
        )
    }

    /// Create a transaction coordinator with custom configuration
    pub fn with_config(
        db: PgPool,
        blockchain_service: Arc<BlockchainService>,
        settlement: Arc<SettlementService>,
        _validation_service: Arc<TransactionValidationService>,
        config: TransactionMonitoringConfig,
    ) -> Self {
        // Initialize sub-services
        let query_service = TransactionQueryService::new(db.clone());
        let monitor_service =
            TransactionMonitorService::new(db.clone(), blockchain_service.clone(), config.clone());
        let recovery_service = TransactionRecoveryService::new(
            db.clone(),
            settlement.clone(),
            query_service.clone(),
            config.clone(),
        );

        Self {
            query_service,
            monitor_service,
            recovery_service,
            config,
        }
    }

    /// Get transaction status by operation ID
    pub async fn get_transaction_status(
        &self,
        operation_id: Uuid,
    ) -> Result<TransactionResponse, ApiError> {
        self.query_service
            .get_transaction_status(operation_id)
            .await
    }

    /// Get transactions for a specific user
    pub async fn get_user_transactions(
        &self,
        user_id: Uuid,
        filters: TransactionFilters,
    ) -> Result<Vec<TransactionResponse>, ApiError> {
        self.query_service
            .get_user_transactions(user_id, filters)
            .await
    }

    /// Get transactions with filters
    pub async fn get_transactions(
        &self,
        filters: TransactionFilters,
    ) -> Result<Vec<TransactionResponse>, ApiError> {
        self.query_service.get_transactions(filters).await
    }

    /// Get transaction statistics
    pub async fn get_transaction_stats(&self) -> Result<TransactionStats, ApiError> {
        self.query_service.get_transaction_stats().await
    }

    /// Monitor pending transactions and update their status
    pub async fn monitor_pending_transactions(&self) -> Result<usize, ApiError> {
        self.monitor_service.monitor_pending_transactions().await
    }

    /// Retry failed transactions
    pub async fn retry_failed_transactions(&self, max_attempts: i32) -> Result<usize, ApiError> {
        self.recovery_service
            .retry_failed_transactions(max_attempts)
            .await
    }

    /// Retry a specific transaction
    pub async fn retry_transaction(
        &self,
        request: TransactionRetryRequest,
    ) -> Result<TransactionRetryResponse, ApiError> {
        self.recovery_service.retry_transaction(request).await
    }

    /// Helper method to get blockchain operation by ID
    pub async fn get_blockchain_operation(
        &self,
        operation_id: Uuid,
    ) -> Result<BlockchainOperation, ApiError> {
        self.query_service
            .get_blockchain_operation(operation_id)
            .await
    }
}
