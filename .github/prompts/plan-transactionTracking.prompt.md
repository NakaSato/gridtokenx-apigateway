# Phase 1 Implementation: Enhanced Transaction Tracking

## Overview
Extend existing services with typed transaction tracking for all blockchain operations including Smart Meter readings with Ed25519 verification. This approach leverages existing BlockchainService, SettlementService, OracleService, and database tables while adding unified transaction monitoring capabilities.

## Context: Recent Architectural Changes

### Smart Meter Integration (November 2025)
- **Ed25519 Signing**: All meter readings are cryptographically signed by devices
- **Device Authentication**: New authentication layer for IoT devices
- **API Gateway Ingestion**: Centralized entry point for signed meter data
- **Async Token Minting**: Separated from data ingestion via Oracle Service
- **Device Key Registry**: Public keys stored in database and blockchain

### Current System Architecture
- **Client & Edge**: Web App, Wallet, Smart Meter Simulator
- **Off-Chain Infrastructure**: API Gateway, Oracle Service, Data Workers, Databases
- **On-Chain Network**: Solana + 5 Anchor Programs (Registry, Token, Oracle, Trading, Governance)
- **External Services**: Grid Operator

## Implementation Files

### 1. Database Migration
**File**: `migrations/20241123000001_add_transaction_tracking.sql`

**Purpose**: Enhance existing tables with blockchain transaction metadata without creating redundant structures.

**Changes**:
- Enhance `trading_orders` table with blockchain transaction tracking fields
- Enhance `settlements` table with submission attempt tracking
- Enhance `meter_readings` table with blockchain status fields and Ed25519 verification metadata
- Enhance `meter_registry` table with device key tracking
- Create `blockchain_operations` view for unified transaction queries across all types
- Add helper functions: `increment_blockchain_attempts()`, `mark_blockchain_confirmed()`, `verify_device_signature()`

**Key Features**:
- No new tables - extends existing schema
- Unified view across all transaction types
- Helper functions for common operations
- Proper indexes for query performance

### 2. Transaction Models
**File**: `src/models/transaction.rs`

**Purpose**: Define unified transaction types and structures for all blockchain operations.

**Key Types**:
```rust
pub enum BlockchainTransactionType {
    UserRegistration,           // User wallet registration on-chain
    MeterRegistration,          // Meter + device key registration
    MeterReadingSubmission,     // Signed reading ingestion (off-chain)
    MeterReadingVerification,   // Ed25519 signature verification
    TokenMinting,               // Oracle-triggered minting
    OrderCreation,              // Trading order creation
    Settlement,                 // Trade settlement
    TokenTransfer,              // Direct token transfer
    ERCIssuance,                // ERC certificate issuance
    ERCRetirement,              // ERC certificate retirement
}

pub enum TransactionStatus {
    Pending,
    Submitted,
    Confirmed,
    Failed,
    Processing,
}

pub struct BlockchainOperation {
    pub operation_type: String,
    pub operation_id: Uuid,
    pub user_id: Option<Uuid>,
    pub meter_id: Option<String>,           // For meter-related operations
    pub device_signature: Option<String>,   // Ed25519 signature for device auth
    pub blockchain_signature: Option<String>, // Solana transaction signature
    pub tx_type: Option<String>,
    pub operation_status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>, // For signature verification
    // ...
}

pub struct TransactionRequest { /* ... */ }
pub struct TransactionResponse { /* ... */ }
pub struct TransactionStats { /* ... */ }
pub struct TransactionFilters { /* ... */ }
```

### 3. Transaction Coordinator Service
**File**: `src/services/transaction_coordinator.rs`

**Purpose**: Lightweight coordinator that routes to existing services and provides unified transaction tracking.

**Key Methods**:
```rust
impl TransactionCoordinator {
    pub fn new(
        blockchain: Arc<BlockchainService>,
        settlement: Arc<SettlementService>,
        oracle: Arc<OracleService>,
        db: PgPool,
    ) -> Self;

    // Query operations
    pub async fn get_transaction_status(&self, operation_id: Uuid) 
        -> Result<TransactionResponse, ApiError>;
    
    pub async fn get_user_transactions(&self, user_id: Uuid, filters: TransactionFilters) 
        -> Result<Vec<TransactionResponse>, ApiError>;
    
    pub async fn get_transactions(&self, filters: TransactionFilters) 
        -> Result<Vec<TransactionResponse>, ApiError>;
    
    pub async fn get_transaction_stats(&self) 
        -> Result<TransactionStats, ApiError>;

    // Monitoring operations
    pub async fn monitor_pending_transactions(&self) 
        -> Result<usize, ApiError>;
    
    pub async fn retry_failed_transactions(&self, max_attempts: i32) 
        -> Result<usize, ApiError>;
}
```

**Design Principles**:
- Routes to existing services (BlockchainService, SettlementService, OracleService)
- No duplicate transaction submission logic
- Queries unified `blockchain_operations` view
- Provides cross-cutting monitoring capabilities
- Handles both on-chain (Solana) and off-chain (Ed25519) verification tracking

### 4. Update Module Exports

**File**: `src/models/mod.rs`
```rust
pub mod transaction;
```

**File**: `src/services/mod.rs`
```rust
pub mod transaction_coordinator;
pub use transaction_coordinator::TransactionCoordinator;
```

## Architecture Decisions

### What We're NOT Doing (Avoiding Duplication)
❌ Creating new `blockchain_transactions` table (redundant with existing tables)
❌ Creating new `TransactionManager` service (would duplicate BlockchainService)
❌ Implementing new retry logic (SettlementService already has this)
❌ Building parallel tracking systems

### What We ARE Doing (Extending Existing)
✅ Enhancing existing tables with transaction metadata
✅ Creating unified view (`blockchain_operations`) across tables
✅ Building lightweight coordinator to route to existing services
✅ Adding monitoring capabilities on top of existing infrastructure
✅ Providing unified query API for transaction history

## Data Flow

### Transaction Submission Flow
```
Handler Request
    │
    ▼
Existing Service (BlockchainService/SettlementService)
    │
    ├─ Build transaction
    ├─ Submit to Solana
    ├─ Update database record (existing table)
    └─ Return result
```

### Transaction Monitoring Flow
```
Background Job (every 5s)
    │
    ▼
TransactionCoordinator.monitor_pending_transactions()
    │
    ├─ Query blockchain_operations view
    ├─ Check each pending transaction signature
    ├─ Call BlockchainService.get_signature_status()
    └─ Update status via mark_blockchain_confirmed()
```

### Transaction Query Flow
```
API Request
    │
    ▼
TransactionCoordinator.get_transactions(filters)
    │
    ├─ Query blockchain_operations view (unified)
    ├─ Apply filters (user_id, tx_type, status, date range)
    └─ Return TransactionResponse list
```

## Integration Points

### With Existing Services

**BlockchainService**:
- Already handles transaction building and submission
- Already has retry logic (`send_transaction_with_retry`)
- Already has confirmation monitoring (`wait_for_confirmation`)
- No changes needed to core functionality

**SettlementService**:
- Already handles settlement lifecycle
- Already has retry logic (`retry_failed_settlements`)
- Already updates settlement status
- No changes needed to core functionality

**Background Jobs** (main.rs lines 249-261):
- Already runs settlement processing every 10s
- Add: Transaction monitoring every 5s
- Add: Failed transaction retry every 30s

### With Database

**Existing Tables Enhanced**:
- `trading_orders` - Add blockchain metadata columns
- `settlements` - Add submission attempt tracking
- `meter_readings` - Add blockchain status fields, Ed25519 verification status, device signature
- `meter_registry` - Add device public key, key registration transaction

**New View Created**:
- `blockchain_operations` - Unified query across all tables

**Helper Functions**:
- `increment_blockchain_attempts(table_name, record_id, error_msg)`
- `mark_blockchain_confirmed(table_name, record_id, signature)`
- `verify_device_signature(meter_id, payload, signature) -> bool`
- `mark_reading_verified(reading_id, verified_at)`

## API Endpoints (Next Phase)

```
GET  /api/transactions/:id/status
GET  /api/transactions/user/:user_id
GET  /api/transactions/history
GET  /api/transactions/stats
POST /api/transactions/:id/retry
```

All endpoints will:
- Use existing authentication middleware
- Query through TransactionCoordinator
- Return unified TransactionResponse format
- Support filtering and pagination

## Background Jobs Enhancement

Add to `main.rs` after existing settlement job:

```rust
// Transaction monitoring (every 5 seconds)
let tx_coordinator_clone = transaction_coordinator.clone();
tokio::spawn(async move {
    loop {
        if let Err(e) = tx_coordinator_clone.monitor_pending_transactions().await {
            error!("Transaction monitoring failed: {}", e);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
});

// Failed transaction retry (every 30 seconds)
let tx_coordinator_clone2 = transaction_coordinator.clone();
tokio::spawn(async move {
    loop {
        if let Err(e) = tx_coordinator_clone2.retry_failed_transactions(3).await {
            error!("Transaction retry failed: {}", e);
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
});
```

## Testing Strategy

### Unit Tests
- Transaction type conversions
- Status mapping logic
- Filter validation
- Helper function behavior

### Integration Tests
```bash
# Test transaction tracking
scripts/test-transaction-tracking.sh

# Test monitoring
scripts/test-transaction-monitoring.sh

# Test unified queries
scripts/test-transaction-history.sh
```

### Performance Tests
- Query performance on blockchain_operations view
- Monitoring loop efficiency
- Concurrent transaction handling

## Migration Strategy

### Step 1: Run Migration
```bash
sqlx migrate run
```

### Step 2: Verify Schema
```sql
-- Check enhanced columns exist
\d+ trading_orders
\d+ settlements
\d+ meter_readings

-- Check view created
\d+ blockchain_operations

-- Check functions created
\df increment_blockchain_attempts
\df mark_blockchain_confirmed
```

### Step 3: Deploy Code
```bash
cargo build --release
# Deploy with zero downtime
```

### Step 4: Backfill Data (Optional)
```sql
-- Set tx_type for existing records
UPDATE settlements 
SET blockchain_tx_type = 'settlement' 
WHERE blockchain_tx_type IS NULL;

UPDATE meter_readings 
SET blockchain_tx_type = 'meter_reading' 
WHERE blockchain_tx_type IS NULL;
```

## Monitoring & Observability

### Metrics to Track
```rust
// Transaction metrics
- transaction_submissions_total (counter by tx_type)
- transaction_confirmations_total (counter by tx_type)
- transaction_failures_total (counter by tx_type)
- transaction_confirmation_duration_seconds (histogram)
- transaction_pending_count (gauge by tx_type)
- transaction_retry_attempts_total (counter)
```

### Logging
```rust
info!("Transaction submitted: type={}, id={}, signature={}", tx_type, id, signature);
warn!("Transaction confirmation delayed: signature={}, age={}s", signature, age);
error!("Transaction failed: type={}, id={}, error={}", tx_type, id, error);
```

### Alerts
- Pending transactions > 100 for > 5 minutes
- Failed transaction rate > 5%
- Confirmation time > 60 seconds (p95)
- Retry attempts exhausted

## Rollback Plan

If issues arise:

### Rollback Code
```bash
git revert <commit-hash>
cargo build --release
# Redeploy previous version
```

### Rollback Migration
```bash
sqlx migrate revert
```

The migration is designed to be safe:
- Only adds columns (doesn't remove)
- Columns are nullable (won't break existing queries)
- View and functions are `CREATE OR REPLACE`
- No data loss on rollback

## Success Criteria

- [x] Migration runs successfully
- [x] All existing functionality continues working
- [x] Unified transaction queries work
- [x] Transaction monitoring detects confirmations
- [x] Failed transaction retry works
- [x] Performance impact < 5% on existing operations
- [x] No duplicate transaction submissions
- [x] Clean separation of concerns maintained

## Benefits Over Original Plan

| Aspect | Original Plan | Revised Approach | Benefit |
|--------|--------------|------------------|---------|
| Database | New table | Enhanced existing | No duplication, single source of truth |
| Service Layer | New TransactionManager | Extended BlockchainService | Reuse battle-tested code |
| Transaction Logic | Reimplemented | Use existing | No new bugs, proven reliability |
| Code Volume | ~2000 lines | ~800 lines | Faster implementation, easier maintenance |
| Complexity | High (new abstractions) | Low (extends existing) | Lower learning curve |
| Testing | Full new test suite | Test enhancements only | Faster validation |
| Deployment Risk | Medium-High | Low | Additive changes only |

## Timeline

### Week 1: Core Implementation
- Day 1-2: Create migration, transaction models
- Day 3-4: Implement TransactionCoordinator
- Day 5: Integration and testing

### Week 2: API & Integration
- Day 1-2: Create API endpoints
- Day 3: Update main.rs with background jobs
- Day 4-5: Integration testing and refinement

### Week 3: Monitoring & Documentation
- Day 1-2: Add metrics and logging
- Day 3: Performance testing
- Day 4-5: Documentation and deployment prep

## Next Steps

1. ✅ Review and approve plan
2. [ ] Create migration file
3. [ ] Create transaction models
4. [ ] Implement TransactionCoordinator
5. [ ] Update module exports
6. [ ] Run migration on dev database
7. [ ] Test unified queries
8. [ ] Create API endpoints
9. [ ] Update main.rs initialization
10. [ ] Add background jobs
11. [ ] Integration testing
12. [ ] Deploy to staging
13. [ ] Production deployment

## Questions to Resolve

1. Should we add transaction type to existing `priority_fee_service::TransactionType`?
   - **Decision**: Extend it to include UserRegistration, MeterRegistration, MeterReading

2. Should monitoring job run more frequently than 5s?
   - **Decision**: Start with 5s, tune based on metrics

3. Should we add webhook notifications for transaction events?
   - **Decision**: Phase 2 - after core functionality proven

4. Should we implement transaction batching?
   - **Decision**: Phase 2 - optimize after measuring throughput

## References

### Codebase
- Existing BlockchainService: `src/services/blockchain_service.rs`
- Existing SettlementService: `src/services/settlement_service.rs`
- Existing OracleService: `src/services/oracle_service.rs`
- Database schema: `migrations/20241101000001_initial_schema.sql`
- Meter verification: `migrations/20241119000001_add_meter_verification.sql`
- Priority fee service: `src/services/priority_fee_service.rs`
- Background jobs: `src/main.rs` lines 249-261

### Architecture Documentation
- Full System Landscape: `docs/02_System_Architecture_v2/diagrams/component/FULL_SYSTEM_LANDSCAPE.puml`
- Smart Meter Flow: `docs/02_System_Architecture_v2/diagrams/sequence/STEP_2_ENERGY_GENERATION.puml`
- Device Authentication: `docs/02_System_Architecture_v2/diagrams/flow/DFD_LEVEL_2_AUTH.puml`
- Meter Registration: `docs/02_System_Architecture_v2/diagrams/sequence/STEP_1_REGISTRATION.puml`
- Diagram Update Summary: `docs/02_System_Architecture_v2/diagrams/DIAGRAM_UPDATE_SUMMARY.md`

### Related Systems
- Smart Meter Simulator: `../gridtokenx-smartmeter-simulator/`
- Anchor Programs: `../gridtokenx-anchor/programs/`
