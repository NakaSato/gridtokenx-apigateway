# Plan: Smart Meter Data Feed Specification (API → Blockchain)

A comprehensive specification document outlining the smart meter data ingestion flow—from IoT device submission through API validation to blockchain tokenization—including current implementation gaps, security requirements, and production-readiness roadmap.

## Steps

### 1. Document Current Data Flow Architecture
Map the existing 3-tier flow: Smart Meter → API Gateway (`src/handlers/meters.rs`, `src/services/meter_service.rs`) → Blockchain (`src/services/blockchain_service.rs`). Include request/response schemas, validation rules, and database persistence in `meter_readings` table with minting status tracking.

### 2. Specify Data Structures & Validation Rules
Define comprehensive schemas for `SubmitMeterReadingRequest` and `MeterReadingResponse` with field-level validation (kWh limits: 0-100, timestamp constraints: ≤7 days old, duplicate prevention: ±15 min window). Include database schema alignment with current `kwh_amount`/`minted`/`mint_tx_signature` fields vs. unused legacy columns (`energy_generated`, `meter_id`, etc.).

### 3. Detail Blockchain Integration Pattern
Specify the token minting flow from `mint_from_reading` handler (lines 487-622 in `src/handlers/meters.rs`) through `BlockchainService::mint_energy_tokens()` with Anchor instruction encoding (SHA256 discriminator, u64 amount encoding, PDA derivation for `token_info`). Include authority wallet management, transaction signing patterns, and database state updates post-confirmation.

### 4. Identify Security Gaps & Production Requirements
Document critical missing features: meter device authentication (`meter_signature` field accepted but **not validated**), meter registration/whitelisting, cross-user duplicate detection, ERC certificate on-chain issuance (currently database-only), and grid operator validation integration. Include threat model for fraudulent readings.

### 5. Propose Production-Ready Architecture
Recommend implementation paths for: cryptographic device authentication (Ed25519 signatures from hardware secure elements), real-time IoT ingestion (MQTT/WebSocket), complete ERC blockchain integration (call `governance::issue_erc()` instruction), schema normalization (remove unused columns or implement full capture), and anomaly detection (statistical + ML-based fraud detection).

## Further Considerations

### 1. Schema Evolution Strategy
Should we migrate to the simplified `kwh_amount`-only model (drop 9 unused columns) or implement full smart meter data capture (`energy_generated`, `energy_consumed`, `voltage`, `temperature`, etc.)? 

**Option A**: Clean schema (faster queries, less storage)
**Option B**: Comprehensive telemetry (enables advanced analytics, predictive maintenance)

### 2. Device Authentication Priority
Current implementation allows **any user** to submit arbitrary readings. For production launch, should we:

**Option A**: Block deployment until cryptographic signatures implemented?
**Option B**: Launch with manual admin review workflow?
**Option C**: Implement rate limiting + statistical anomaly detection as interim measure?

### 3. ERC Blockchain Integration Timeline
The governance program's `issue_erc()` instruction exists but API doesn't call it, causing mismatch between database certificates and on-chain validation. Should we:

**Option A**: Complete integration before Phase 5 (trading)?
**Option B**: Mock on-chain checks in trading program for MVP?
**Option C**: Use centralized certificate registry temporarily?

## Research Summary

### Current Implementation Status

#### ✅ Implemented
- **Meter Reading Submission**: API endpoint `POST /api/meters/submit` with JWT auth
- **Validation**: amount, timestamp, duplicates
- **Database storage**: `meter_readings` table with proper indexes
- **Token Minting**: Admin endpoint `POST /api/admin/meters/mint-from-reading`
- **Blockchain transactions**: Anchor instruction encoding and submission
- **Authority wallet management**: Secure keypair loading and caching
- **ERC Certificates**: Database CRUD operations
- **Statistics**: User stats and unminted/minted totals

#### ⚠️ Gaps & Incomplete Features

1. **Smart Meter Authentication**
   - Status: `meter_signature` field exists but **NOT VALIDATED**
   - Impact: No cryptographic proof that reading came from legitimate device
   - Risk: Users can submit arbitrary readings

2. **Meter Device Registration**
   - Status: No meter registration flow
   - Gap: Original schema had `meter_id` field, but current API doesn't use it
   - Missing: Meter-to-user association tracking

3. **Real-Time IoT Integration**
   - Status: Manual submission only via HTTP API
   - Missing: MQTT/WebSocket ingestion for live meter data
   - Missing: Automated reading collection from smart meters
   - Missing: Device authentication (TLS, API keys)

4. **ERC Blockchain Integration**
   - Status: Database operations work, but **no Anchor client calls**
   - Gap: Certificates created in DB don't trigger blockchain `issue_erc()` instruction
   - Impact: Certificates not registered on-chain for trading validation

5. **Duplicate Prevention Weaknesses**
   - Current: ±15 min window check per user
   - Gap: No check across different users for same meter
   - Risk: Multiple users could claim same meter's output

6. **Energy Source Verification**
   - Status: No integration with grid operator data
   - Missing: Cross-reference with utility meter readings
   - Risk: Inflated or fabricated generation claims

7. **Historical Reading Validation**
   - Current: 7-day age limit
   - Gap: No pattern analysis for anomalies (sudden spikes, impossible values)
   - Missing: ML-based fraud detection

### Data Flow Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. Smart Meter Submission (User)                                │
│    POST /api/meters/submit                                       │
│    { kwh_amount, reading_timestamp, meter_signature? }          │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. MeterService Validation                                       │
│    • Amount: 0 < kwh ≤ 100                                      │
│    • Timestamp: not future, < 7 days old                        │
│    • Duplicate check: ±15 min window per user                   │
│    • ⚠️ meter_signature: ACCEPTED BUT NOT VALIDATED             │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. Database Storage (meter_readings table)                       │
│    • user_id, wallet_address, kwh_amount                        │
│    • reading_timestamp, submitted_at                             │
│    • minted = FALSE (initial state)                             │
│    • mint_tx_signature = NULL                                    │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. Admin Minting Trigger                                         │
│    POST /api/admin/meters/mint-from-reading                     │
│    { reading_id }                                                │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. BlockchainService::mint_energy_tokens()                      │
│    a) Convert kWh to lamports (multiply by 1e9)                │
│    b) Derive token_info PDA: ["token_info"]                    │
│    c) Build Anchor instruction:                                 │
│       - Discriminator: SHA256("global:mint_tokens_direct")[0..8]│
│       - Data: amount as u64 little-endian                       │
│       - Accounts: token_info, mint, user_ata, authority, etc.  │
│    d) Sign with authority keypair                               │
│    e) Submit to Solana RPC                                      │
│    f) Wait for confirmation                                     │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. Update Database                                               │
│    UPDATE meter_readings                                         │
│    SET minted = true, mint_tx_signature = '<signature>'         │
│    WHERE id = <reading_id>                                       │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│ 7. Return Success Response                                       │
│    { success: true, transaction_signature, reading_id, ... }    │
└─────────────────────────────────────────────────────────────────┘
```

### Key Data Structures

#### MeterReading (Database Model)
```rust
pub struct MeterReading {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    pub kwh_amount: Option<BigDecimal>,
    pub reading_timestamp: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub minted: Option<bool>,
    pub mint_tx_signature: Option<String>,
}
```

#### SubmitMeterReadingRequest (API Input)
```rust
pub struct SubmitMeterReadingRequest {
    pub wallet_address: String,
    pub kwh_amount: BigDecimal,
    pub reading_timestamp: DateTime<Utc>,
    pub meter_signature: Option<String>, // ⚠️ Not validated
}
```

#### MeterReadingResponse (API Output)
```rust
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub wallet_address: String,
    pub kwh_amount: BigDecimal,
    pub reading_timestamp: DateTime<Utc>,
    pub submitted_at: DateTime<Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
}
```

### Schema Mismatch: Database vs. Implementation

| Database Column | Used in Code? | Notes |
|-----------------|---------------|-------|
| `meter_id` | ❌ | Defined in schema, never populated |
| `energy_generated` | ❌ | Not used (replaced by `kwh_amount`) |
| `energy_consumed` | ❌ | Not used |
| `surplus_energy` | ❌ | Not used |
| `deficit_energy` | ❌ | Not used |
| `battery_level` | ❌ | Not used |
| `temperature` | ❌ | Not used |
| `voltage` | ❌ | Not used |
| `current` | ❌ | Not used |
| `kwh_amount` | ✅ | Main data field |
| `user_id` | ✅ | Links to users table |
| `minted` | ✅ | Tracks tokenization status |
| `mint_tx_signature` | ✅ | Stores blockchain proof |

**Conclusion**: Initial schema was comprehensive but API uses simplified model. Decision needed on schema evolution strategy.

### Key Files Reference

| Component | File Path | Lines | Description |
|-----------|-----------|-------|-------------|
| **Handlers** |
| Submit Reading | `src/handlers/meters.rs` | 196-259 | User submission endpoint |
| Mint from Reading | `src/handlers/meters.rs` | 487-622 | Admin minting endpoint |
| **Services** |
| MeterService | `src/services/meter_service.rs` | 33-468 | Reading validation & storage |
| BlockchainService | `src/services/blockchain_service.rs` | 305-373 | Token minting logic |
| WalletService | `src/services/wallet_service.rs` | 14-280 | Authority keypair management |
| **Models** |
| MeterReading | `src/services/meter_service.rs` | 9-18 | Active database model |
| EnergyReading | `src/models/energy.rs` | 7-18 | Unused comprehensive model |
| **Database** |
| Initial Schema | `migrations/20241101000001_initial_schema.sql` | 140-160 | Original meter_readings table |

### Production Readiness Recommendations

#### High Priority

1. **Implement Meter Device Authentication**
   - Use cryptographic signatures (Ed25519) from hardware secure elements
   - Validate `meter_signature` in `MeterService::validate_reading()`
   - Store meter public keys in new `meters` table

2. **Complete ERC Blockchain Integration**
   - Implement Anchor client calls in `ErcService::issue_certificate()`
   - Call `governance::issue_erc()` instruction
   - Update `MeterAccount.claimed_erc_generation` on-chain

3. **Add Meter Registration Flow**
   - Create `POST /api/meters/register` endpoint
   - Link meters to users in database
   - Store meter public keys for signature validation

#### Medium Priority

4. **Implement Cross-User Duplicate Detection**
   - Check if reading timestamp already exists for given meter across all users
   - Prevent multiple users claiming same meter output

5. **Add Real-Time Ingestion**
   - MQTT broker for IoT devices
   - WebSocket endpoint for streaming data
   - Message queue (RabbitMQ/Kafka) for high-volume ingestion

6. **Schema Cleanup**
   - Migrate unused columns OR implement full capture
   - Add `meter_id` foreign key to meters table
   - Normalize schema for better data integrity

#### Low Priority

7. **Anomaly Detection**
   - Implement statistical outlier detection
   - Flag readings >2 std deviations from user's average
   - Integrate ML model for fraud detection

8. **Grid Operator Integration**
   - Add middleware to validate readings against utility APIs
   - Cache grid data for offline validation
   - Implement reconciliation jobs

### Security Risks Summary

⚠️ **Critical Risks**:
- Users can submit arbitrary readings (no device proof)
- No meter-to-user binding enforcement
- Certificates not validated on-chain for trading
- Cross-user duplicate prevention missing

**Current State**: Suitable for MVP/testing, but requires significant hardening for production use in a regulated energy market.
