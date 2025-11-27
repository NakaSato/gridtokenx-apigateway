# Transaction Flow: API Gateway to Blockchain (Anchor Programs)

**Document Version:** 1.0  
**Last Updated:** November 24, 2025  
**Status:** Implementation Plan

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Transaction Flow Phases](#transaction-flow-phases)
4. [Component Details](#component-details)
5. [Implementation Phases](#implementation-phases)
6. [API Endpoints](#api-endpoints)
7. [Blockchain Programs](#blockchain-programs)
8. [Data Flow Diagrams](#data-flow-diagrams)
9. [Error Handling](#error-handling)
10. [Monitoring & Observability](#monitoring--observability)

---

## Overview

This document outlines the complete transaction flow from the API Gateway (Rust/Axum) to Solana blockchain Anchor programs, covering validation, coordination, submission, monitoring, settlement, and post-processing.

### Key Components

- **API Gateway**: Rust-based REST API (Axum framework)
- **Transaction Coordinator**: Central orchestration service
- **Blockchain Service**: Solana RPC client wrapper
- **Anchor Programs**: On-chain smart contracts (Registry, Oracle, Trading, Energy Token, Governance)
- **Settlement Service**: Post-transaction processing
- **Monitoring Service**: Real-time status tracking

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         API Layer (Axum)                        │
├─────────────────────────────────────────────────────────────────┤
│  HTTP Handlers → Validation → Authentication → Authorization   │
└────────────────────────────┬────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────────┐
│                   Transaction Coordinator                       │
├─────────────────────────────────────────────────────────────────┤
│  • Transaction Creation    • Status Tracking                    │
│  • Validation Service      • Retry Logic                        │
│  • Instruction Building    • Monitoring                         │
└────────────────────────────┬────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────────┐
│                    Blockchain Service                           │
├─────────────────────────────────────────────────────────────────┤
│  • RPC Client              • Transaction Signing                │
│  • Priority Fees           • Status Polling                     │
│  • Blockhash Management    • Confirmation Tracking              │
└────────────────────────────┬────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────────┐
│                  Solana Blockchain Network                      │
├─────────────────────────────────────────────────────────────────┤
│  Anchor Programs:                                               │
│  • Registry (Users/Meters)  • Trading (Orders/Matching)         │
│  • Oracle (Data Feed)       • Energy Token (Minting/Transfer)   │
│  • Governance (ERC/PoA)                                         │
└────────────────────────────┬────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────────┐
│                    Post-Processing Layer                        │
├─────────────────────────────────────────────────────────────────┤
│  • Settlement Service      • Webhook Notifications              │
│  • Reconciliation          • Analytics                          │
│  • Audit Logging           • Event Publishing                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Transaction Flow Phases

### Phase 1: Request & Validation

```
User Request → API Handler → JWT Auth → Request Validation
     ↓
Transaction Validation Service:
  • Input validation (amounts, addresses, signatures)
  • Business rule validation (balance checks, limits)
  • User authorization (permissions, roles)
  • Resource validation (meter existence, ERC validity)
     ↓
Database State Check (PostgreSQL)
```

### Phase 2: Transaction Creation

```
Transaction Coordinator
     ↓
Instruction Builder (by transaction type):
  • User Registration → Registry Program
  • Meter Registration → Registry Program
  • Meter Reading → Oracle Program
  • Order Creation → Trading Program
  • Settlement → Energy Token Program
  • ERC Issuance → Governance Program
     ↓
Transaction Assembly:
  • Get recent blockhash
  • Calculate priority fees
  • Build instruction data
  • Add required accounts
  • Create transaction object
```

### Phase 3: Submission & Monitoring

```
Blockchain Service
     ↓
Transaction Submission:
  • Sign transaction with authority keypair
  • Submit to RPC endpoint
  • Get transaction signature
     ↓
Database Recording:
  • Store signature in blockchain_operations table
  • Update status to 'submitted'
  • Record submission timestamp
     ↓
Background Monitoring Task:
  • Poll transaction status every 5 seconds
  • Check confirmation status
  • Handle expiration (5 minutes timeout)
  • Update database with status changes
```

### Phase 4: Confirmation & Settlement

```
Blockchain Confirmation:
  • Transaction included in block
  • 32 block confirmations (~13 seconds on Solana)
     ↓
Status Update:
  • Mark as 'confirmed' in database
  • Record confirmation timestamp
  • Trigger settlement service
     ↓
Settlement Processing:
  • Execute post-transaction logic
  • Update account balances
  • Create settlement records
  • Emit events for analytics
```

---

## Component Details

### 1. Transaction Validation Service

**Location:** `src/services/transaction_validation_service.rs` (to be created)

**Responsibilities:**
- Validate transaction inputs (amounts, addresses, data)
- Verify digital signatures
- Check business rules and constraints
- Validate user permissions
- Ensure sufficient balances/resources

**Key Methods:**
```rust
pub async fn validate_transaction_request(
    &self,
    request: &TransactionRequest,
) -> Result<ValidationResult, ValidationError>

pub async fn validate_user_registration(
    &self,
    user_data: &UserRegistration,
) -> Result<(), ValidationError>

pub async fn validate_order_creation(
    &self,
    order: &CreateOrderRequest,
    user_id: Uuid,
) -> Result<(), ValidationError>

pub async fn validate_settlement(
    &self,
    settlement_id: Uuid,
) -> Result<(), ValidationError>
```

### 2. Transaction Coordinator (Extended)

**Location:** `src/services/transaction_coordinator.rs` (exists, to be extended)

**New Methods:**
```rust
// Phase 1: Transaction Creation
pub async fn create_transaction(
    &self,
    tx_type: BlockchainTransactionType,
    payload: TransactionPayload,
    user_id: Uuid,
) -> Result<TransactionResponse, ApiError>

// Build instructions for specific transaction types
pub async fn build_user_registration_instruction(
    &self,
    user: &UserRegistration,
) -> Result<Instruction, ApiError>

pub async fn build_order_creation_instruction(
    &self,
    order: &CreateOrderRequest,
) -> Result<Instruction, ApiError>

pub async fn build_settlement_instruction(
    &self,
    settlement: &Settlement,
) -> Result<Instruction, ApiError>

// Submit transaction to blockchain
pub async fn submit_transaction(
    &self,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
) -> Result<Signature, ApiError>
```

### 3. Instruction Builders

**Location:** `src/services/instruction_builders/` (to be created)

```
instruction_builders/
├── mod.rs
├── registry_instructions.rs      // User/Meter registration
├── oracle_instructions.rs         // Meter readings, market clearing
├── trading_instructions.rs        // Order creation/matching
├── energy_token_instructions.rs   // Minting, transfers
└── governance_instructions.rs     // ERC issuance, PoA
```

**Example: Registry Instructions**
```rust
pub struct RegistryInstructionBuilder {
    program_id: Pubkey,
    registry: Pubkey,
}

impl RegistryInstructionBuilder {
    pub fn register_user(
        &self,
        user_authority: Pubkey,
        user_type: UserType,
        location: String,
    ) -> Result<Instruction> {
        // Build account metas
        let accounts = vec![
            AccountMeta::new(registry, false),
            AccountMeta::new(user_account_pda, false),
            AccountMeta::new(user_authority, true),
            AccountMeta::new_readonly(system_program, false),
        ];
        
        // Build instruction data
        let data = RegisterUserData {
            user_type,
            location,
        };
        
        // Create instruction
        Ok(Instruction {
            program_id: self.program_id,
            accounts,
            data: data.try_to_vec()?,
        })
    }
}
```

### 4. Blockchain Service (Extended)

**Location:** `src/services/blockchain_service.rs` (exists)

**Program IDs:**
```rust
REGISTRY_PROGRAM_ID:      2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7
ORACLE_PROGRAM_ID:        DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE
GOVERNANCE_PROGRAM_ID:    4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe
ENERGY_TOKEN_PROGRAM_ID:  94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur
TRADING_PROGRAM_ID:       GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk
```

**Enhanced Methods:**
```rust
// Priority fee calculation
pub async fn calculate_priority_fee(
    &self,
    tx_type: TransactionType,
) -> Result<u64>

// Transaction simulation
pub async fn simulate_transaction(
    &self,
    transaction: &Transaction,
) -> Result<SimulationResult>

// Batch transaction submission
pub async fn submit_batch_transactions(
    &self,
    transactions: Vec<Transaction>,
) -> Result<Vec<Signature>>
```

### 5. Settlement Service (Enhanced)

**Location:** `src/services/settlement_service.rs` (exists, to be enhanced)

**New Capabilities:**
```rust
// Handle different settlement types
pub async fn settle_order(
    &self,
    order_id: Uuid,
) -> Result<SettlementTransaction>

pub async fn settle_meter_reading(
    &self,
    reading_id: Uuid,
) -> Result<SettlementTransaction>

// Automatic settlement on confirmation
pub async fn auto_settle_confirmed_transaction(
    &self,
    operation_id: Uuid,
) -> Result<()>

// Financial reconciliation
pub async fn reconcile_settlements(
    &self,
    date: DateTime<Utc>,
) -> Result<ReconciliationReport>
```

### 6. Monitoring Service

**Location:** `src/services/transaction_monitoring_service.rs` (to be created)

**Background Tasks:**
```rust
// Periodic status checks
pub async fn monitor_pending_transactions(&self) -> Result<usize>

// Expiration handling
pub async fn handle_expired_transactions(&self) -> Result<usize>

// Automatic retry
pub async fn retry_failed_transactions(
    &self,
    max_attempts: i32,
) -> Result<usize>

// Health checks
pub async fn check_blockchain_health(&self) -> Result<HealthStatus>
```

### 7. Webhook Service

**Location:** `src/services/webhook_service.rs` (to be created)

**Notification Types:**
```rust
pub enum WebhookEvent {
    TransactionSubmitted { signature: String, ... },
    TransactionConfirmed { signature: String, ... },
    TransactionFailed { error: String, ... },
    SettlementCompleted { settlement_id: Uuid, ... },
}

pub async fn notify_transaction_status(
    &self,
    user_id: Uuid,
    event: WebhookEvent,
) -> Result<()>
```

---

## Implementation Phases

### **Phase 1: Core Transaction Flow** (Week 1-2)

#### 1.1 Create Transaction Validation Service
- [ ] Create `src/services/transaction_validation_service.rs`
- [ ] Implement input validation methods
- [ ] Add business rule validation
- [ ] Create validation error types
- [ ] Add unit tests

#### 1.2 Extend Transaction Coordinator
- [ ] Add `create_transaction()` method
- [ ] Add `submit_transaction()` method
- [ ] Implement transaction state management
- [ ] Add error recovery logic

#### 1.3 Create Instruction Builders
- [ ] Create `src/services/instruction_builders/` module
- [ ] Implement `RegistryInstructionBuilder`
- [ ] Implement `OracleInstructionBuilder`
- [ ] Implement `TradingInstructionBuilder`
- [ ] Implement `EnergyTokenInstructionBuilder`
- [ ] Implement `GovernanceInstructionBuilder`

#### 1.4 Create Transaction API Endpoints
- [ ] POST `/api/v1/transactions/create` - Create new transaction
- [ ] GET `/api/v1/transactions/:id` - Get transaction details
- [ ] POST `/api/v1/transactions/:id/submit` - Submit transaction
- [ ] Update OpenAPI documentation

**Deliverables:**
- Working transaction creation flow
- Validated transaction submission
- Basic instruction builders for all program types

---

### **Phase 2: Monitoring & Status Tracking** (Week 3-4)

#### 2.1 Enhanced Transaction Monitoring
- [ ] Create `src/services/transaction_monitoring_service.rs`
- [ ] Implement real-time status polling
- [ ] Add WebSocket support for live updates
- [ ] Integrate with existing `TransactionCoordinator`

#### 2.2 Background Monitoring Task
- [ ] Create background task scheduler
- [ ] Implement periodic status checks (5-second intervals)
- [ ] Add confirmation tracking (32 blocks)
- [ ] Monitor transaction pool

#### 2.3 Transaction Status Webhooks
- [ ] Create `src/services/webhook_service.rs`
- [ ] Implement webhook registration endpoints
- [ ] Add event-driven notifications
- [ ] Create webhook retry logic

#### 2.4 Expiration Handling
- [ ] Add expiration time tracking
- [ ] Implement automatic timeout (5 minutes)
- [ ] Create expired transaction cleanup
- [ ] Add retry with new blockhash

**Deliverables:**
- Real-time transaction monitoring
- Webhook notifications
- Automatic expiration handling
- WebSocket status updates

---

### **Phase 3: Settlement & Post-Processing** (Week 5-6)

#### 3.1 Enhanced Settlement Service
- [ ] Extend settlement for all transaction types
- [ ] Add `settle_order()` method
- [ ] Add `settle_meter_reading()` method
- [ ] Add `settle_erc_issuance()` method

#### 3.2 Post-Transaction Processing Hooks
- [ ] Create event system for transaction lifecycle
- [ ] Add `on_transaction_confirmed()` hook
- [ ] Add `on_transaction_failed()` hook
- [ ] Add `on_settlement_completed()` hook

#### 3.3 Automatic Settlement
- [ ] Implement auto-settlement trigger
- [ ] Add settlement queue
- [ ] Create batch settlement processor
- [ ] Add settlement retry logic

#### 3.4 Financial Reconciliation
- [ ] Create reconciliation service
- [ ] Add daily reconciliation reports
- [ ] Implement balance verification
- [ ] Add discrepancy detection

**Deliverables:**
- Automatic settlement for confirmed transactions
- Event-driven post-processing
- Financial reconciliation system
- Settlement reports

---

### **Phase 4: Advanced Features** (Week 7-8)

#### 4.1 Transaction Batching
- [ ] Implement batch transaction creation
- [ ] Add batch submission endpoint
- [ ] Optimize for compute unit efficiency
- [ ] Add batch status tracking

#### 4.2 Priority Fee Optimization
- [ ] Integrate with `PriorityFeeService`
- [ ] Add dynamic fee calculation
- [ ] Implement fee estimation API
- [ ] Add congestion-based adjustments

#### 4.3 Analytics & Reporting
- [ ] Create analytics service
- [ ] Add transaction metrics collection
- [ ] Implement performance dashboards
- [ ] Add success rate tracking

#### 4.4 Transaction Rollback
- [ ] Implement compensation transactions
- [ ] Add rollback for failed settlements
- [ ] Create reversal instructions
- [ ] Add rollback audit trail

**Deliverables:**
- Batch transaction processing
- Dynamic priority fees
- Comprehensive analytics
- Transaction rollback mechanisms

---

## API Endpoints

### Transaction Management

#### Create Transaction
```http
POST /api/v1/transactions/create
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "tx_type": "order_creation",
  "payload": {
    "order_side": "sell",
    "energy_amount": 100,
    "price_per_kwh": 150,
    "erc_certificate_id": "erc_123"
  }
}

Response 201 Created:
{
  "operation_id": "uuid",
  "tx_type": "order_creation",
  "status": "pending",
  "created_at": "2025-11-24T10:00:00Z"
}
```

#### Submit Transaction
```http
POST /api/v1/transactions/:id/submit
Authorization: Bearer <jwt_token>

Response 200 OK:
{
  "operation_id": "uuid",
  "signature": "4xZ3...",
  "status": "submitted",
  "submitted_at": "2025-11-24T10:00:05Z"
}
```

#### Get Transaction Status
```http
GET /api/v1/transactions/:id/status
Authorization: Bearer <jwt_token>

Response 200 OK:
{
  "operation_id": "uuid",
  "operation_type": "trading_order",
  "tx_type": "order_creation",
  "status": "confirmed",
  "signature": "4xZ3...",
  "attempts": 1,
  "created_at": "2025-11-24T10:00:00Z",
  "submitted_at": "2025-11-24T10:00:05Z",
  "confirmed_at": "2025-11-24T10:00:18Z"
}
```

#### Get User Transactions
```http
GET /api/v1/transactions/user?status=confirmed&limit=50&offset=0
Authorization: Bearer <jwt_token>

Response 200 OK:
{
  "transactions": [...],
  "total": 150,
  "page": 1,
  "page_size": 50
}
```

#### Retry Failed Transaction
```http
POST /api/v1/transactions/:id/retry
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "operation_type": "settlements",
  "max_attempts": 5
}

Response 200 OK:
{
  "success": true,
  "attempts": 2,
  "signature": "5yA4...",
  "status": "submitted"
}
```

#### Get Transaction Statistics
```http
GET /api/v1/transactions/stats
Authorization: Bearer <jwt_token>

Response 200 OK:
{
  "total_count": 10000,
  "pending_count": 25,
  "submitted_count": 50,
  "confirmed_count": 9800,
  "failed_count": 125,
  "processing_count": 50,
  "avg_confirmation_time_seconds": 13.5,
  "success_rate": 0.987
}
```

### Batch Operations

#### Create Batch Transactions
```http
POST /api/v1/transactions/batch/create
Authorization: Bearer <jwt_token>
Content-Type: application/json

{
  "transactions": [
    {
      "tx_type": "meter_reading",
      "payload": {...}
    },
    {
      "tx_type": "token_minting",
      "payload": {...}
    }
  ]
}

Response 201 Created:
{
  "batch_id": "uuid",
  "transactions": [
    { "operation_id": "uuid1", "status": "pending" },
    { "operation_id": "uuid2", "status": "pending" }
  ]
}
```

---

## Blockchain Programs

### 1. Registry Program

**Program ID:** `2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7`

**Instructions:**
- `initialize` - Initialize registry with authority
- `register_user` - Register new user (Producer/Consumer/Prosumer)
- `register_meter` - Register smart meter for user
- `update_user_status` - Update user status (admin)
- `update_meter_status` - Update meter status (admin)

**Accounts:**
- `Registry` - Global registry state
- `UserAccount` - Individual user account
- `MeterAccount` - Individual meter account

### 2. Oracle Program

**Program ID:** `DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE`

**Instructions:**
- `initialize` - Initialize oracle with API gateway authority
- `submit_meter_reading` - Submit verified meter reading
- `trigger_market_clearing` - Trigger market clearing process
- `update_oracle_status` - Update oracle configuration
- `update_api_gateway` - Update API gateway authority

**Accounts:**
- `OracleConfig` - Oracle configuration
- `MeterReading` - Individual meter reading record

### 3. Trading Program

**Program ID:** `GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk`

**Instructions:**
- `initialize` - Initialize trading program
- `initialize_market` - Create trading market
- `create_sell_order` - Create sell order (requires ERC)
- `create_buy_order` - Create buy order
- `match_orders` - Match buy/sell orders
- `settle_trade` - Settle matched trade
- `cancel_order` - Cancel open order

**Accounts:**
- `Market` - Trading market state
- `Order` - Individual order account
- `TradeMatch` - Matched trade record

### 4. Energy Token Program

**Program ID:** `94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur`

**Instructions:**
- `initialize` - Initialize token program
- `create_token_mint` - Create GRID token mint
- `mint_to_wallet` - Mint tokens to user wallet
- `transfer_tokens` - Transfer tokens between users
- `burn_tokens` - Burn tokens (for settlements)

**Accounts:**
- `TokenMint` - GRID token mint account
- `TokenAccount` - User token accounts

### 5. Governance Program

**Program ID:** `4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe`

**Instructions:**
- `initialize_poa` - Initialize Proof of Authority
- `issue_erc` - Issue Energy Renewable Certificate
- `validate_erc_for_trading` - Validate ERC for trading
- `emergency_pause` - Emergency pause (PoA authority)
- `emergency_unpause` - Emergency unpause
- `update_governance_config` - Update configuration

**Accounts:**
- `GovernanceConfig` - Global governance state
- `ErcCertificate` - Individual ERC certificate

---

## Data Flow Diagrams

### User Registration Flow

```
┌──────────┐     ┌────────────┐     ┌─────────────────┐     ┌──────────┐
│          │     │            │     │  Transaction    │     │          │
│  Client  │────▶│ API Handler│────▶│  Coordinator    │────▶│ Registry │
│          │     │            │     │                 │     │ Program  │
└──────────┘     └────────────┘     └─────────────────┘     └──────────┘
     │                 │                      │                    │
     │  POST /users    │                      │                    │
     │  + JWT          │   Validate Request   │                    │
     ├────────────────▶│                      │                    │
     │                 │                      │                    │
     │                 │   Create Transaction │                    │
     │                 ├─────────────────────▶│                    │
     │                 │                      │                    │
     │                 │                      │ Build Instruction  │
     │                 │                      │ register_user()    │
     │                 │                      ├───────────────────▶│
     │                 │                      │                    │
     │                 │                      │ Submit Transaction │
     │                 │                      │ + Signature        │
     │                 │                      ├───────────────────▶│
     │                 │                      │                    │
     │                 │   Transaction Created│   Process & Emit   │
     │                 │◀─────────────────────│      Event         │
     │                 │                      │◀───────────────────│
     │  201 Created    │                      │                    │
     │◀────────────────│                      │                    │
     │  + operation_id │                      │                    │
```

### Order Creation Flow

```
┌──────────┐     ┌────────────┐     ┌─────────────────┐     ┌──────────┐
│          │     │            │     │  Transaction    │     │          │
│  Client  │────▶│ API Handler│────▶│  Coordinator    │────▶│ Trading  │
│          │     │            │     │                 │     │ Program  │
└──────────┘     └────────────┘     └─────────────────┘     └──────────┘
     │                 │                      │                    │
     │  POST /orders   │                      │                    │
     │  + order_data   │   Validate Order     │                    │
     ├────────────────▶│   • Check balance    │                    │
     │                 │   • Verify ERC       │                    │
     │                 │   • Check limits     │                    │
     │                 │                      │                    │
     │                 │   Build Instruction  │                    │
     │                 │   create_sell_order()│                    │
     │                 ├─────────────────────▶│                    │
     │                 │                      │                    │
     │                 │                      │ Validate ERC       │
     │                 │                      │ Create Order       │
     │                 │                      ├───────────────────▶│
     │                 │                      │                    │
     │                 │   Order Created      │   Match Orders     │
     │                 │◀─────────────────────│   (if applicable)  │
     │  201 Created    │                      │◀───────────────────│
     │◀────────────────│                      │                    │
     │  + order_id     │                      │                    │
```

### Settlement Flow

```
┌─────────────────┐     ┌─────────────┐     ┌──────────────┐
│   Transaction   │     │  Settlement │     │ Energy Token │
│   Coordinator   │────▶│   Service   │────▶│   Program    │
└─────────────────┘     └─────────────┘     └──────────────┘
         │                      │                    │
         │ Order Confirmed      │                    │
         │ (on-chain event)     │                    │
         ├─────────────────────▶│                    │
         │                      │                    │
         │                      │ Calculate Amounts  │
         │                      │ • Energy tokens    │
         │                      │ • Platform fees    │
         │                      │ • Net amounts      │
         │                      │                    │
         │                      │ Build Transfer     │
         │                      │ Instructions       │
         │                      ├───────────────────▶│
         │                      │                    │
         │                      │ Transfer Tokens    │
         │                      │ Seller → Buyer     │
         │                      │◀───────────────────│
         │   Settlement Done    │                    │
         │◀─────────────────────│                    │
         │                      │                    │
         │ Update Database      │                    │
         │ Send Webhooks        │                    │
```

### Monitoring Flow

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│  Background  │     │   Transaction   │     │  Blockchain  │
│     Task     │────▶│   Coordinator   │────▶│   Service    │
└──────────────┘     └─────────────────┘     └──────────────┘
      │                      │                       │
      │ Every 5 seconds      │                       │
      │                      │                       │
      │ Get Pending TXs      │                       │
      ├─────────────────────▶│                       │
      │                      │                       │
      │                      │ For each pending TX:  │
      │                      │ Get signature status  │
      │                      ├──────────────────────▶│
      │                      │                       │
      │                      │   Query RPC endpoint  │
      │                      │   Check confirmations │
      │                      │◀──────────────────────│
      │                      │                       │
      │   Status: Confirmed  │                       │
      │◀─────────────────────│                       │
      │                      │                       │
      │ Update Database      │                       │
      │ Trigger Settlement   │                       │
      │ Send Webhook         │                       │
```

---

## Error Handling

### Error Types

```rust
pub enum TransactionError {
    // Validation errors
    ValidationFailed(String),
    InsufficientBalance,
    InvalidSignature,
    UnauthorizedAccess,
    
    // Blockchain errors
    TransactionFailed(String),
    InstructionError(u32),
    SimulationFailed(String),
    InsufficientComputeUnits,
    
    // Network errors
    RpcError(String),
    TimeoutError,
    NetworkCongestion,
    
    // Business logic errors
    OrderNotFound,
    ErcExpired,
    MarketClosed,
    InsufficientLiquidity,
}
```

### Error Recovery Strategies

| Error Type | Recovery Strategy | Max Retries |
|-----------|------------------|-------------|
| Network timeout | Retry with exponential backoff | 3 |
| Blockhash expired | Get new blockhash and retry | 2 |
| Insufficient compute | Increase compute units | 1 |
| Simulation failed | Validate inputs and retry | 1 |
| RPC error | Switch to backup RPC | 2 |
| Transaction expired | Create new transaction | 0 |

### Retry Logic

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_backoff: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            exponential_backoff: true,
        }
    }
}
```

---

## Monitoring & Observability

### Metrics to Track

1. **Transaction Metrics**
   - Total transactions created
   - Transactions by type
   - Transactions by status
   - Average confirmation time
   - Success rate
   - Failure rate by error type

2. **Performance Metrics**
   - RPC response time
   - Transaction submission latency
   - Confirmation latency
   - Settlement processing time
   - API endpoint response time

3. **Business Metrics**
   - Trading volume
   - Token minting rate
   - User registration rate
   - Order match rate
   - Settlement success rate

### Logging Strategy

```rust
// Transaction lifecycle logging
info!("Transaction created: {}", operation_id);
debug!("Building instruction for tx type: {}", tx_type);
info!("Transaction submitted: {} (signature: {})", operation_id, signature);
warn!("Transaction pending for {} seconds: {}", duration, operation_id);
info!("Transaction confirmed: {} (slot: {})", operation_id, slot);
error!("Transaction failed: {} (error: {})", operation_id, error);
```

### Alerting Rules

- Alert if pending transactions > 100
- Alert if confirmation time > 60 seconds
- Alert if failure rate > 5%
- Alert if RPC health check fails
- Alert if settlement queue > 50

---

## Database Schema

### blockchain_operations (Unified View)

```sql
CREATE VIEW blockchain_operations AS
SELECT
    'trading_order' AS operation_type,
    id AS operation_id,
    user_id,
    transaction_hash AS signature,
    'order_creation' AS tx_type,
    status::text AS operation_status,
    0 AS attempts,
    NULL AS last_error,
    created_at AS submitted_at,
    settled_at AS confirmed_at,
    created_at,
    updated_at
FROM trading_orders
WHERE transaction_hash IS NOT NULL

UNION ALL

SELECT
    'settlement' AS operation_type,
    id AS operation_id,
    buyer_id AS user_id,
    blockchain_tx AS signature,
    'settlement' AS tx_type,
    blockchain_status AS operation_status,
    blockchain_attempts AS attempts,
    blockchain_last_error AS last_error,
    blockchain_submitted_at AS submitted_at,
    confirmed_at,
    created_at,
    updated_at
FROM settlements
WHERE blockchain_tx IS NOT NULL;
```

### Helper Functions

```sql
-- Mark transaction as submitted
CREATE FUNCTION mark_blockchain_submitted(
    table_name TEXT,
    record_id UUID,
    signature TEXT,
    tx_type TEXT
) RETURNS BOOLEAN;

-- Mark transaction as confirmed
CREATE FUNCTION mark_blockchain_confirmed(
    table_name TEXT,
    record_id UUID,
    signature TEXT,
    status TEXT
) RETURNS BOOLEAN;

-- Increment attempt counter
CREATE FUNCTION increment_blockchain_attempts(
    table_name TEXT,
    record_id UUID,
    error_message TEXT
) RETURNS BOOLEAN;
```

---

## Testing Strategy

### Unit Tests
- Validation logic
- Instruction building
- Error handling
- Retry logic

### Integration Tests
- API endpoint flows
- Database operations
- Transaction submission
- Settlement processing

### End-to-End Tests
- Complete user registration flow
- Order creation and matching flow
- Settlement flow
- Monitoring and retry flow

### Performance Tests
- Transaction throughput
- Concurrent transaction handling
- Database query performance
- RPC client performance

---

## Security Considerations

1. **Private Key Management**
   - Store authority keypairs securely (AWS Secrets Manager, HashiCorp Vault)
   - Use separate keypairs for different environments
   - Rotate keys periodically

2. **Transaction Signing**
   - Sign transactions server-side
   - Never expose private keys to clients
   - Validate all transaction parameters

3. **Access Control**
   - Verify JWT tokens on all endpoints
   - Check user permissions for operations
   - Audit all transaction attempts

4. **Rate Limiting**
   - Limit transaction creation per user
   - Prevent spam and abuse
   - Monitor for suspicious activity

5. **Input Validation**
   - Validate all inputs server-side
   - Sanitize user-provided data
   - Check business rules before submission

---

## Performance Optimization

### Transaction Batching
- Combine multiple instructions in single transaction
- Reduce RPC calls
- Optimize compute units

### Caching Strategy
- Cache blockhash for 60 seconds
- Cache program accounts
- Cache user balances

### Connection Pooling
- Maintain persistent RPC connections
- Use connection pool for database
- Reuse HTTP clients

### Compute Unit Optimization
- Calculate required compute units
- Set appropriate limits
- Use priority fees during congestion

---

## Deployment Checklist

- [ ] Environment variables configured
- [ ] Database migrations applied
- [ ] Authority keypairs deployed
- [ ] RPC endpoints configured
- [ ] Monitoring dashboards set up
- [ ] Alerting rules configured
- [ ] Webhook endpoints tested
- [ ] Load testing completed
- [ ] Security audit completed
- [ ] Documentation updated

---

## Future Enhancements

1. **Multi-signature Support**
   - Add multi-sig authority for critical operations
   - Implement approval workflows

2. **Cross-Program Invocations**
   - Combine multiple program calls in single transaction
   - Atomic multi-program operations

3. **Versioned Transactions**
   - Support Solana v0 transactions
   - Enable address lookup tables

4. **Advanced Analytics**
   - Real-time dashboards
   - Predictive failure detection
   - Cost optimization recommendations

5. **Automated Governance**
   - On-chain voting for parameter changes
   - Automated fee adjustments
   - Self-healing mechanisms

---

## Glossary

- **Anchor**: Solana framework for building smart contracts
- **Blockhash**: Recent block identifier used in transactions
- **Compute Units**: Solana's measurement of transaction complexity
- **CPI**: Cross-Program Invocation
- **ERC**: Energy Renewable Certificate
- **Instruction**: Single operation in a Solana transaction
- **Lamports**: Smallest unit of SOL (1 SOL = 1 billion lamports)
- **PDA**: Program Derived Address
- **PoA**: Proof of Authority
- **RPC**: Remote Procedure Call (Solana node API)
- **Settlement**: Post-trade processing and token transfer
- **Signature**: Transaction identifier on Solana
- **Slot**: Solana's unit of time (~400ms)

---

## References

- [Solana Documentation](https://docs.solana.com/)
- [Anchor Framework](https://www.anchor-lang.com/)
- [Solana Cookbook](https://solanacookbook.com/)
- [GridTokenX Architecture](../README.md)

---

**Document Status:** Ready for Implementation  
**Next Steps:** Begin Phase 1 Implementation  
**Owner:** Development Team  
**Review Cycle:** Weekly
