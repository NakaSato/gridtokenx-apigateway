# Transaction Flow Implementation Plan: API to Blockchain

## Overview

This document outlines a comprehensive plan for implementing a complete transaction flow from API to blockchain in the GridTokenX energy trading system. The plan extends the existing architecture with new components and workflows to handle end-to-end transaction processing.

## Current Architecture

The system consists of two main components:
- **gridtokenx-anchor**: Solana smart contracts for energy trading, including programs for trading, tokens, registry, oracle, and governance
- **gridtokenx-apigateway**: Rust API gateway that interfaces with the blockchain, already containing a `TransactionCoordinator`, `BlockchainService`, and various transaction handlers

## Transaction Flow Components

### 1. Transaction Creation & Validation

#### 1.1. Enhanced Transaction Models

```rust
// src/models/transaction.rs
pub enum TransactionType {
    EnergyTrade,
    TokenMint,
    TokenTransfer,
    GovernanceVote,
    OracleUpdate,
    RegistryUpdate,
}

pub struct CreateTransactionRequest {
    pub transaction_type: TransactionType,
    pub user_id: Uuid,
    pub payload: TransactionPayload,
    pub max_priority_fee: Option<u64>,
    pub skip_prevalidation: bool,
}

pub enum TransactionPayload {
    EnergyTrade {
        market_pubkey: String,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: OrderType,
        erc_certificate_id: Option<String>,
    },
    TokenMint {
        recipient: String,
        amount: u64,
    },
    // ... other payload types
}
```

#### 1.2. Transaction Validation Service

```rust
// src/services/transaction_validation_service.rs
pub struct TransactionValidationService {
    erc_service: Arc<ErcService>,
    market_service: Arc<MarketService>,
    token_service: Arc<TokenService>,
}

impl TransactionValidationService {
    pub async fn validate_transaction(&self, request: &CreateTransactionRequest) -> Result<(), ValidationError> {
        match request.transaction_type {
            TransactionType::EnergyTrade => self.validate_energy_trade(&request.payload).await,
            TransactionType::TokenMint => self.validate_token_mint(&request.payload).await,
            // ... other validations
        }
    }
    
    async fn validate_energy_trade(&self, payload: &TransactionPayload) -> Result<(), ValidationError> {
        // Validate ERC certificate if provided
        // Check market status
        // Verify energy amount and price
        // Ensure user has necessary permissions
    }
}
```

### 2. Transaction Submission API

#### 2.1. API Endpoint for Creating Transactions

```rust
// src/handlers/transactions/create.rs
#[utoipa::path(
    post,
    path = "/api/v1/transactions",
    tag = "transactions",
    summary = "Create and submit a blockchain transaction",
    request_body(content = CreateTransactionRequest),
    responses(
        (status = 202, description = "Transaction accepted for processing", body = TransactionResponse),
        (status = 400, description = "Invalid transaction request"),
        (status = 500, description = "Internal server error")
    ),
    security(("jwt" = []))
)]
pub async fn create_transaction(
    State(app_state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(request): Json<CreateTransactionRequest>,
) -> Result<Json<TransactionResponse>, ApiError> {
    // 1. Validate transaction
    app_state.validation_service
        .validate_transaction(&request)
        .await?;
    
    // 2. Create transaction record
    let transaction = app_state.transaction_coordinator
        .create_transaction(user.sub, request)
        .await?;
    
    // 3. Submit to blockchain asynchronously
    let coordinator = app_state.transaction_coordinator.clone();
    tokio::spawn(async move {
        if let Err(e) = coordinator.submit_to_blockchain(transaction.operation_id).await {
            error!("Failed to submit transaction {}: {}", transaction.operation_id, e);
        }
    });
    
    Ok(Json(transaction))
}
```

#### 2.2. Transaction Coordinator Extension

```rust
// src/services/transaction_coordinator.rs
impl TransactionCoordinator {
    pub async fn create_transaction(
        &self,
        user_id: Uuid,
        request: CreateTransactionRequest,
    ) -> Result<TransactionResponse, ApiError> {
        // Create transaction record in database
        let operation_id = Uuid::new_v4();
        
        // Store initial transaction state
        sqlx::query!(
            "INSERT INTO blockchain_operations (...) VALUES (...)",
            operation_id,
            user_id,
            request.transaction_type as i32,
            TransactionStatus::Pending as i32,
            // ... other fields
        )
        .execute(&self.db)
        .await?;
        
        self.get_transaction_status(operation_id).await
    }
    
    pub async fn submit_to_blockchain(&self, operation_id: Uuid) -> Result<(), ApiError> {
        // Get transaction details
        let operation = self.get_blockchain_operation(operation_id).await?;
        
        // Build transaction based on type
        let transaction = match operation.operation_type {
            TransactionType::EnergyTrade => self.build_energy_trade_tx(&operation).await?,
            TransactionType::TokenMint => self.build_token_mint_tx(&operation).await?,
            // ... other transaction types
        };
        
        // Submit transaction
        let signature = self.blockchain_service
            .submit_transaction(transaction)
            .await?;
        
        // Update transaction with signature
        self.update_transaction_signature(operation_id, signature).await?;
        
        Ok(())
    }
    
    async fn build_energy_trade_tx(&self, operation: &BlockchainOperation) -> Result<Transaction, ApiError> {
        // Deserialize payload
        let payload: EnergyTradePayload = serde_json::from_value(operation.payload.clone())?;
        
        // Build instruction for trading program
        let instruction = build_create_order_instruction(
            TRADING_PROGRAM_ID,
            payload.market_pubkey,
            payload.energy_amount,
            payload.price_per_kwh,
            payload.order_type,
            payload.erc_certificate_id,
        )?;
        
        // Create transaction
        let mut transaction = Transaction::new_with_payer(
            &[instruction],
            Some(&self.blockchain_service.payer_pubkey()),
        );
        
        // Add priority fee if specified
        if let Some(fee) = operation.max_priority_fee {
            self.blockchain_service
                .add_priority_fee(&mut transaction, TransactionType::EnergyTrade, fee)?;
        }
        
        Ok(transaction)
    }
}
```

### 3. Smart Contract Interaction Layer

#### 3.1. Instruction Builders for Smart Contracts

```rust
// src/services/blockchain_service.rs
impl BlockchainService {
    pub fn build_create_order_instruction(
        &self,
        market_pubkey: &str,
        energy_amount: u64,
        price_per_kwh: u64,
        order_type: OrderType,
        erc_certificate_id: Option<String>,
    ) -> Result<Instruction> {
        // Build the instruction for the trading program
        let program_id = Pubkey::from_str(TRADING_PROGRAM_ID)?;
        let market = Pubkey::from_str(market_pubkey)?;
        
        // Generate new order account
        let order_keypair = Keypair::new();
        let order_pubkey = order_keypair.pubkey();
        
        // Build accounts array
        let accounts = vec![
            AccountMeta::new(market, false),
            AccountMeta::new(order_pubkey, false),
            // ... other required accounts
        ];
        
        // Build instruction data
        let mut data = Vec::new();
        data.extend_from_slice(&(energy_amount as u64).to_le_bytes());
        data.extend_from_slice(&(price_per_kwh as u64).to_le_bytes());
        data.push(order_type as u8);
        // ... add other data fields
        
        Ok(Instruction {
            program_id,
            accounts,
            data,
        })
    }
}
```

### 4. Transaction Monitoring & Confirmation

#### 4.1. Enhanced Transaction Monitoring

```rust
// src/services/transaction_coordinator.rs
impl TransactionCoordinator {
    pub async fn monitor_pending_transactions(&self) -> Result<(), ApiError> {
        // Get all pending transactions
        let pending_operations = sqlx::query!(
            "SELECT * FROM blockchain_operations WHERE status = $1",
            TransactionStatus::Pending as i32
        )
        .fetch_all(&self.db)
        .await?;
        
        // Process each pending transaction
        for operation in pending_operations {
            if let Some(signature) = operation.signature {
                // Check transaction status on blockchain
                let confirmed = self.blockchain_service
                    .confirm_transaction(&signature)
                    .await?;
                
                if confirmed {
                    self.mark_transaction_confirmed(operation.operation_id).await?;
                } else if self.is_transaction_expired(&operation) {
                    self.mark_transaction_failed(operation.operation_id, "Transaction expired").await?;
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn start_background_monitoring(&self) -> Result<(), ApiError> {
        let coordinator = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                if let Err(e) = coordinator.monitor_pending_transactions().await {
                    error!("Error monitoring transactions: {}", e);
                }
            }
        });
        
        Ok(())
    }
}
```

### 5. Settlement & Post-Transaction Processing

#### 5.1. Enhanced Settlement Service

```rust
// src/services/settlement_service.rs
impl SettlementService {
    pub async fn process_settlement(&self, operation_id: Uuid) -> Result<(), ApiError> {
        // Get transaction details
        let operation = self.get_transaction(operation_id).await?;
        
        match operation.operation_type {
            TransactionType::EnergyTrade => {
                // Process energy trade settlement
                self.settle_energy_trade(&operation).await?;
                
                // Update user token balances
                self.update_token_balances(&operation).await?;
                
                // Record trade completion
                self.record_trade_completion(&operation).await?;
            }
            TransactionType::TokenMint => {
                // Process token minting
                self.process_token_mint(&operation).await?;
            }
            // ... other settlement types
        }
        
        Ok(())
    }
    
    async fn settle_energy_trade(&self, operation: &BlockchainOperation) -> Result<(), ApiError> {
        // Get trade details from the blockchain
        let trade_record = self.blockchain_service
            .get_trade_record(operation.signature.as_ref().unwrap())
            .await?;
        
        // Calculate settlement amounts
        let buyer_payment = trade_record.total_value + trade_record.fee_amount;
        let seller_payment = trade_record.total_value - trade_record.fee_amount;
        
        // Process financial settlement
        self.process_financial_settlement(
            trade_record.buyer,
            trade_record.seller,
            buyer_payment,
            seller_payment,
        ).await?;
        
        Ok(())
    }
}
```

### 6. Error Handling & Retry Mechanism

#### 6.1. Advanced Retry Logic

```rust
// src/services/transaction_coordinator.rs
impl TransactionCoordinator {
    pub async fn retry_transaction(&self, request: TransactionRetryRequest) -> Result<TransactionRetryResponse, ApiError> {
        // Get transaction details
        let operation = self.get_blockchain_operation(request.operation_id).await?;
        
        // Check if retry is allowed
        if operation.attempts >= self.config.max_retry_attempts {
            return Err(ApiError::BadRequest("Transaction has exceeded maximum retry attempts".to_string()));
        }
        
        // Increment attempt count
        self.increment_attempt_count(request.operation_id).await?;
        
        // Resubmit transaction with updated priority fee
        let signature = self.resubmit_with_priority_fee(&operation).await?;
        
        // Update transaction with new signature
        self.update_transaction_signature(request.operation_id, signature).await?;
        
        Ok(TransactionRetryResponse {
            success: true,
            attempts: operation.attempts + 1,
            new_signature: Some(signature),
            message: "Transaction resubmitted successfully".to_string(),
        })
    }
    
    async fn resubmit_with_priority_fee(&self, operation: &BlockchainOperation) -> Result<Signature, ApiError> {
        // Build transaction again with higher priority fee
        let mut transaction = self.build_transaction_from_operation(operation).await?;
        
        // Add higher priority fee
        let priority_fee = self.calculate_priority_fee(operation.attempts + 1);
        self.blockchain_service
            .add_priority_fee(&mut transaction, operation.operation_type, priority_fee)?;
        
        // Submit transaction
        self.blockchain_service
            .submit_transaction(transaction)
            .await
    }
}
```

## Implementation Roadmap

### Phase 1: Core Transaction Flow (Weeks 1-3)
1. **Week 1: Transaction Models & Validation**
   - Implement `TransactionValidationService`
   - Extend transaction models with new fields
   - Create validation rules for each transaction type

2. **Week 2: Transaction Coordinator Enhancement**
   - Extend `TransactionCoordinator` with transaction creation and submission
   - Implement transaction building for each type
   - Add database schema updates for new transaction fields

3. **Week 3: API Endpoints & Smart Contract Integration**
   - Create transaction creation API endpoint
   - Implement basic smart contract instruction builders
   - Add transaction submission workflow

### Phase 2: Monitoring & Status Tracking (Weeks 4-5)
1. **Week 4: Transaction Monitoring**
   - Implement enhanced transaction monitoring
   - Add background monitoring task
   - Create transaction status webhook notifications

2. **Week 5: Transaction Lifecycle Management**
   - Implement transaction expiration handling
   - Add transaction status history tracking
   - Create transaction metrics collection

### Phase 3: Settlement & Post-Processing (Weeks 6-7)
1. **Week 6: Settlement Service Enhancement**
   - Enhance `SettlementService` for different transaction types
   - Implement post-transaction processing hooks
   - Add automatic settlement for confirmed transactions

2. **Week 7: Financial Integration**
   - Implement financial reconciliation
   - Add token balance updates
   - Create settlement reporting

### Phase 4: Advanced Features (Weeks 8-10)
1. **Week 8: Transaction Optimization**
   - Add transaction batching for efficiency
   - Implement priority fee optimization
   - Create transaction routing logic

2. **Week 9: Analytics & Reporting**
   - Add transaction analytics and reporting
   - Create transaction performance metrics
   - Implement transaction cost analysis

3. **Week 10: Resilience & Reliability**
   - Implement transaction rollback mechanisms
   - Add circuit breaker pattern for blockchain interactions
   - Create transaction replay capabilities

## Technical Considerations

### 1. Database Schema Updates
The following tables will need to be updated or created:
- `blockchain_operations`: Extended with new fields for transaction payload, priority fee, etc.
- `transaction_events`: New table for tracking transaction lifecycle events
- `settlement_records`: Enhanced with fields for different transaction types

### 2. Error Handling Strategy
- Implement comprehensive error classification (network, validation, blockchain, etc.)
- Add detailed error logging for debugging
- Create error recovery patterns for each error type

### 3. Performance Optimizations
- Implement transaction batching for high-volume operations
- Add caching for frequently accessed blockchain data
- Use connection pooling for database and blockchain RPC connections

### 4. Security Considerations
- Implement proper access controls for transaction types
- Add transaction signing verification
- Create audit trails for all transaction modifications

## Success Metrics

1. **Transaction Success Rate**: Target >95% successful transactions
2. **Average Confirmation Time**: Target <30 seconds for normal transactions
3. **System Availability**: Target >99.9% uptime
4. **API Response Time**: Target <200ms for transaction submission

## Testing Strategy

1. **Unit Tests**: Test individual components in isolation
2. **Integration Tests**: Test interaction between components
3. **End-to-End Tests**: Test complete transaction flows
4. **Load Tests**: Test system under high transaction volume
5. **Chaos Tests**: Test system resilience under failure conditions

## Monitoring & Observability

1. **Transaction Metrics**: Track transaction success rate, confirmation time, retry attempts
2. **Performance Metrics**: Monitor API response times, blockchain interaction latency
3. **Error Metrics**: Track error rates, error types, and recovery success
4. **Business Metrics**: Track energy trading volume, token minting volume, etc.

## Conclusion

This implementation plan provides a comprehensive framework for handling transactions from API to blockchain in the GridTokenX energy trading system. The phased approach allows for incremental development and testing while maintaining system stability.

The plan leverages the existing architecture while adding new capabilities for transaction validation, monitoring, and settlement. The implementation will provide a robust, scalable, and reliable transaction processing system for energy trading on the Solana blockchain.