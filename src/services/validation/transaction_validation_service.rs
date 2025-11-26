// Transaction Validation Service
// Validates transaction requests before submission to blockchain

use crate::error::ApiError;
use crate::models::transaction::{
    CreateTransactionRequest, EnergyTradePayload, GovernanceVotePayload, OracleUpdatePayload,
    RegistryUpdatePayload, TokenMintPayload, TokenTransferPayload, TransactionPayload,
    ValidationError,
};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

/// Services needed for transaction validation
pub trait ValidationServices: Send + Sync {
    fn get_erc_certificate(&self, certificate_id: &str) -> Result<ErcCertificate, ApiError>;
    fn get_market(&self, market_pubkey: &str) -> Result<Market, ApiError>;
    fn get_user_balance(&self, user_id: &str, token_mint: &str) -> Result<u64, ApiError>;
    fn is_user_eligible_to_vote(&self, user_id: Uuid, proposal_id: u64) -> Result<bool, ApiError>;
    fn has_oracle_authority(&self, user_id: Uuid) -> Result<bool, ApiError>;
    fn has_registry_authority(&self, user_id: Uuid, participant_id: &str)
    -> Result<bool, ApiError>;
}

/// ERC Certificate information
#[derive(Debug, Clone)]
pub struct ErcCertificate {
    pub certificate_id: String,
    pub energy_amount: u64,
    pub validated_for_trading: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Market information
#[derive(Debug, Clone)]
pub struct Market {
    pub market_pubkey: String,
    pub active: bool,
    pub market_fee_bps: u32,
}

/// Transaction Validation Service
#[derive(Clone)]
pub struct TransactionValidationService {
    services: Arc<dyn ValidationServices>,
}

impl TransactionValidationService {
    /// Create a new transaction validation service
    pub fn new(services: Arc<dyn ValidationServices>) -> Self {
        Self { services }
    }

    /// Validate a transaction request
    pub async fn validate_transaction(
        &self,
        request: &CreateTransactionRequest,
    ) -> Result<(), ValidationError> {
        debug!(
            "Validating transaction of type: {:?}",
            request.transaction_type
        );

        // Skip prevalidation if requested
        if request.skip_prevalidation {
            debug!("Skipping prevalidation as requested");
            return Ok(());
        }

        match &request.payload {
            TransactionPayload::EnergyTrade { market_pubkey, energy_amount, price_per_kwh, order_type, erc_certificate_id } => {
                let payload = EnergyTradePayload {
                    market_pubkey: market_pubkey.clone(),
                    energy_amount: *energy_amount,
                    price_per_kwh: *price_per_kwh,
                    order_type: order_type.clone(),
                    erc_certificate_id: erc_certificate_id.clone(),
                };
                self.validate_energy_trade(request.user_id, &payload).await
            }
            TransactionPayload::TokenMint { recipient, amount } => {
                let payload = TokenMintPayload {
                    recipient: recipient.clone(),
                    amount: *amount,
                };
                self.validate_token_mint(request.user_id, &payload).await
            }
            TransactionPayload::TokenTransfer { from, to, amount, token_mint } => {
                let payload = TokenTransferPayload {
                    from: from.clone(),
                    to: to.clone(),
                    amount: *amount,
                    token_mint: token_mint.clone(),
                };
                self.validate_token_transfer(request.user_id, &payload).await
            }
            TransactionPayload::GovernanceVote { proposal_id, vote } => {
                let payload = GovernanceVotePayload {
                    proposal_id: *proposal_id,
                    vote: *vote,
                };
                self.validate_governance_vote(request.user_id, &payload)
                    .await
            }
            TransactionPayload::OracleUpdate { price_feed_id, price, confidence } => {
                let payload = OracleUpdatePayload {
                    price_feed_id: price_feed_id.clone(),
                    price: *price,
                    confidence: *confidence,
                };
                self.validate_oracle_update(request.user_id, &payload).await
            }
            TransactionPayload::RegistryUpdate { participant_id, update_data } => {
                let payload = RegistryUpdatePayload {
                    participant_id: participant_id.clone(),
                    update_data: update_data.clone(),
                };
                self.validate_registry_update(request.user_id, &payload)
                    .await
            }
        }
    }

    /// Validate energy trade transaction
    async fn validate_energy_trade(
        &self,
        _user_id: Uuid,
        payload: &EnergyTradePayload,
    ) -> Result<(), ValidationError> {
        // Validate energy amount
        if payload.energy_amount == 0 {
            return Err(ValidationError::new(
                "INVALID_ENERGY_AMOUNT",
                "Energy amount must be greater than 0",
            ));
        }

        // Validate price
        if payload.price_per_kwh == 0 {
            return Err(ValidationError::new(
                "INVALID_PRICE",
                "Price per kWh must be greater than 0",
            ));
        }

        // Validate market status
        let market = self
            .services
            .get_market(&payload.market_pubkey)
            .map_err(|e| {
                ValidationError::new("MARKET_ERROR", &format!("Failed to fetch market: {}", e))
            })?;

        if !market.active {
            return Err(ValidationError::new(
                "MARKET_INACTIVE",
                "Market is not active for trading",
            ));
        }

        // Validate ERC certificate if provided
        if let Some(cert_id) = &payload.erc_certificate_id {
            let cert = self.services.get_erc_certificate(cert_id).map_err(|e| {
                ValidationError::new(
                    "ERC_ERROR",
                    &format!("Failed to fetch ERC certificate: {}", e),
                )
            })?;

            // Check certificate status
            if !cert.validated_for_trading {
                return Err(ValidationError::new(
                    "ERC_NOT_VALIDATED",
                    "ERC certificate is not validated for trading",
                ));
            }

            // Check certificate expiration
            if let Some(expires_at) = cert.expires_at {
                if expires_at < chrono::Utc::now() {
                    return Err(ValidationError::new(
                        "ERC_EXPIRED",
                        "ERC certificate has expired",
                    ));
                }
            }

            // Verify energy amount doesn't exceed certificate amount
            if payload.energy_amount > cert.energy_amount {
                return Err(ValidationError::new(
                    "EXCEEDS_ERC_AMOUNT",
                    "Energy amount exceeds ERC certificate amount",
                ));
            }
        } else {
            warn!("No ERC certificate provided for energy trade - may fail validation on chain");
        }

        debug!("Energy trade validation passed");
        Ok(())
    }

    /// Validate token mint transaction
    async fn validate_token_mint(
        &self,
        user_id: Uuid,
        payload: &TokenMintPayload,
    ) -> Result<(), ValidationError> {
        // Validate recipient address format
        if payload.recipient.is_empty() {
            return Err(ValidationError::new(
                "INVALID_RECIPIENT",
                "Recipient address cannot be empty",
            ));
        }

        // Validate amount
        if payload.amount == 0 {
            return Err(ValidationError::new(
                "INVALID_AMOUNT",
                "Token amount must be greater than 0",
            ));
        }

        // Check if user has minting authority (simplified check)
        // In a real implementation, this would check user roles/permissions
        if user_id.to_string().chars().count() == 0 {
            return Err(ValidationError::new(
                "UNAUTHORIZED",
                "User not authorized to mint tokens",
            ));
        }

        debug!("Token mint validation passed");
        Ok(())
    }

    /// Validate token transfer transaction
    async fn validate_token_transfer(
        &self,
        _user_id: Uuid,
        payload: &TokenTransferPayload,
    ) -> Result<(), ValidationError> {
        // Validate addresses
        if payload.from.is_empty() {
            return Err(ValidationError::with_field(
                "INVALID_SENDER",
                "Sender address cannot be empty",
                "from",
            ));
        }

        if payload.to.is_empty() {
            return Err(ValidationError::with_field(
                "INVALID_RECIPIENT",
                "Recipient address cannot be empty",
                "to",
            ));
        }

        if payload.from == payload.to {
            return Err(ValidationError::new(
                "SELF_TRANSFER",
                "Cannot transfer tokens to the same address",
            ));
        }

        // Validate amount
        if payload.amount == 0 {
            return Err(ValidationError::new(
                "INVALID_AMOUNT",
                "Token amount must be greater than 0",
            ));
        }

        // Check sender balance
        let balance = self
            .services
            .get_user_balance(&payload.from, &payload.token_mint)
            .map_err(|e| {
                ValidationError::new("BALANCE_ERROR", &format!("Failed to fetch balance: {}", e))
            })?;

        if balance < payload.amount {
            return Err(ValidationError::new(
                "INSUFFICIENT_BALANCE",
                "Insufficient token balance for transfer",
            ));
        }

        debug!("Token transfer validation passed");
        Ok(())
    }

    /// Validate governance vote transaction
    async fn validate_governance_vote(
        &self,
        user_id: Uuid,
        payload: &GovernanceVotePayload,
    ) -> Result<(), ValidationError> {
        // Validate proposal ID
        if payload.proposal_id == 0 {
            return Err(ValidationError::new(
                "INVALID_PROPOSAL_ID",
                "Proposal ID must be greater than 0",
            ));
        }

        // Check if user is eligible to vote
        let eligible = self
            .services
            .is_user_eligible_to_vote(user_id, payload.proposal_id)
            .map_err(|e| {
                ValidationError::new(
                    "VOTING_ERROR",
                    &format!("Failed to check voting eligibility: {}", e),
                )
            })?;

        if !eligible {
            return Err(ValidationError::new(
                "NOT_ELIGIBLE_TO_VOTE",
                "User is not eligible to vote on this proposal",
            ));
        }

        debug!("Governance vote validation passed");
        Ok(())
    }

    /// Validate oracle update transaction
    async fn validate_oracle_update(
        &self,
        user_id: Uuid,
        payload: &OracleUpdatePayload,
    ) -> Result<(), ValidationError> {
        // Validate price feed ID
        if payload.price_feed_id.is_empty() {
            return Err(ValidationError::new(
                "INVALID_PRICE_FEED_ID",
                "Price feed ID cannot be empty",
            ));
        }

        // Validate price
        if payload.price == 0 {
            return Err(ValidationError::new(
                "INVALID_PRICE",
                "Price must be greater than 0",
            ));
        }

        // Validate confidence (0-100%)
        if payload.confidence > 100 {
            return Err(ValidationError::new(
                "INVALID_CONFIDENCE",
                "Confidence must be between 0 and 100",
            ));
        }

        // Check if user has oracle authority
        let has_authority = self.services.has_oracle_authority(user_id).map_err(|e| {
            ValidationError::new(
                "ORACLE_ERROR",
                &format!("Failed to check oracle authority: {}", e),
            )
        })?;

        if !has_authority {
            return Err(ValidationError::new(
                "UNAUTHORIZED_ORACLE",
                "User does not have oracle authority",
            ));
        }

        debug!("Oracle update validation passed");
        Ok(())
    }

    /// Validate registry update transaction
    async fn validate_registry_update(
        &self,
        user_id: Uuid,
        payload: &RegistryUpdatePayload,
    ) -> Result<(), ValidationError> {
        // Validate participant ID
        if payload.participant_id.is_empty() {
            return Err(ValidationError::new(
                "INVALID_PARTICIPANT_ID",
                "Participant ID cannot be empty",
            ));
        }

        // Validate update data (basic check)
        if payload.update_data.is_null() {
            return Err(ValidationError::new(
                "INVALID_UPDATE_DATA",
                "Update data cannot be null",
            ));
        }

        // Check if user has registry authority for this participant
        let has_authority = self
            .services
            .has_registry_authority(user_id, &payload.participant_id)
            .map_err(|e| {
                ValidationError::new(
                    "REGISTRY_ERROR",
                    &format!("Failed to check registry authority: {}", e),
                )
            })?;

        if !has_authority {
            return Err(ValidationError::new(
                "UNAUTHORIZED_REGISTRY",
                "User does not have authority to update this participant",
            ));
        }

        debug!("Registry update validation passed");
        Ok(())
    }
}
