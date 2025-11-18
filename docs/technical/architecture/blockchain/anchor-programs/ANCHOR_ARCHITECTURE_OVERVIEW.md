# GridTokenX Blockchain Architecture Overview

**Document Version:** 1.0  
**Last Updated:** November 7, 2025  
**Status:** Active

## Table of Contents

1. [Introduction](#introduction)
2. [Architecture Overview](#architecture-overview)
3. [Program Architecture](#program-architecture)
4. [Data Flow](#data-flow)
5. [Security Model](#security-model)
6. [Integration Patterns](#integration-patterns)
7. [Deployment](#deployment)

---

## Introduction

The GridTokenX platform utilizes Solana blockchain through Anchor framework to implement a decentralized peer-to-peer energy trading system. This document provides a comprehensive overview of the blockchain architecture, including all five Anchor programs, their interactions, and data flows.

### Key Technologies

- **Blockchain:** Solana (High-performance L1)
- **Framework:** Anchor 0.32.1
- **Runtime:** Solana Program Library (SPL)
- **Network:** Localnet (Development), Devnet/Mainnet (Production)

### Design Principles

1. **Modularity:** Separation of concerns across five specialized programs
2. **Security:** Program-derived addresses (PDAs) and authority validation
3. **Efficiency:** Optimized account structures and cross-program invocations (CPI)
4. **Auditability:** Event emission for all state changes
5. **Upgradability:** Governance-controlled program updates

---

## Architecture Overview

### System Context

```
┌─────────────────────────────────────────────────────────────┐
│                    GridTokenX Ecosystem                      │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────┐         ┌──────────────────┐              │
│  │ API Gateway │────────▶│  Oracle Program  │              │
│  │  (Rust)     │         │  (Anchor)        │              │
│  └─────────────┘         └──────────────────┘              │
│         │                          │                         │
│         │                          ▼                         │
│         │                 ┌──────────────────┐              │
│         │                 │ Registry Program │              │
│         │                 │   (Anchor)       │              │
│         │                 └──────────────────┘              │
│         │                          │                         │
│         │          ┌───────────────┼───────────────┐        │
│         │          │               │               │         │
│         ▼          ▼               ▼               ▼         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         Energy Token       Trading       Governance  │   │
│  │           Program          Program         Program   │   │
│  │          (Anchor)         (Anchor)        (Anchor)   │   │
│  └─────────────────────────────────────────────────────┘   │
│                            │                                 │
│                            ▼                                 │
│                   ┌────────────────┐                        │
│                   │ Solana Ledger  │                        │
│                   │  (Blockchain)  │                        │
│                   └────────────────┘                        │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Five Core Programs

| Program ID | Program Name | Primary Purpose | Deploy Address |
|------------|--------------|-----------------|----------------|
| `94G1r674LmRD...` | **Energy Token** | GRID token management & minting | energy_token.so |
| `2XPQmFYMdXjP...` | **Registry** | User & meter registration | registry.so |
| `DvdtU4quEbux...` | **Oracle** | External data ingestion | oracle.so |
| `GZnqNTJsre6q...` | **Trading** | P2P energy marketplace | trading.so |
| `4DY97YYBt4bx...` | **Governance** | PoA & ERC certification | governance.so |

---

## Program Architecture

### 1. Registry Program
**Program ID:** `2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7`

#### Purpose
Central registration hub for users, meters, and energy data tracking. Acts as the source of truth for identity and energy measurements.

#### Key Instructions

```rust
pub mod registry {
    // Initialization
    pub fn initialize(ctx, authority) -> Result<()>
    
    // User Management
    pub fn register_user(ctx, user_type, location) -> Result<()>
    pub fn update_user_status(ctx, new_status) -> Result<()>
    pub fn is_valid_user(ctx) -> Result<bool>
    
    // Meter Management
    pub fn register_meter(ctx, meter_id, meter_type) -> Result<()>
    pub fn update_meter_reading(ctx, generated, consumed, timestamp) -> Result<()>
    pub fn is_valid_meter(ctx) -> Result<bool>
    
    // Tokenization Interface
    pub fn get_unsettled_balance(ctx) -> Result<u64>
    pub fn settle_meter_balance(ctx) -> Result<u64>
}
```

#### Account Structure

**Registry Account** (PDA: `["registry"]`)
```rust
pub struct Registry {
    pub authority: Pubkey,      // Admin authority
    pub user_count: u64,        // Total registered users
    pub meter_count: u64,       // Total registered meters
    pub created_at: i64,        // Initialization timestamp
}
```

**UserAccount** (PDA: `["user", user_authority]`)
```rust
pub struct UserAccount {
    pub authority: Pubkey,      // Owner wallet
    pub user_type: UserType,    // Prosumer/Consumer
    pub location: String,       // Geographic location (max 100 chars)
    pub status: UserStatus,     // Active/Suspended/Inactive
    pub registered_at: i64,     // Registration timestamp
    pub meter_count: u32,       // Number of owned meters
}

pub enum UserType { Prosumer, Consumer }
pub enum UserStatus { Active, Suspended, Inactive }
```

**MeterAccount** (PDA: `["meter", meter_id]`)
```rust
pub struct MeterAccount {
    pub meter_id: String,               // Unique identifier (max 50 chars)
    pub owner: Pubkey,                  // User who owns this meter
    pub meter_type: MeterType,          // Solar/Wind/Battery/Grid
    pub status: MeterStatus,            // Active/Inactive/Maintenance
    pub registered_at: i64,             // Registration timestamp
    pub last_reading_at: i64,           // Last update timestamp
    
    // Energy Tracking
    pub total_generation: u64,          // Cumulative generated (Wh)
    pub total_consumption: u64,         // Cumulative consumed (Wh)
    
    // Tokenization Prevention (Double-Spend Protection)
    pub settled_net_generation: u64,    // Already minted as GRID tokens
    pub claimed_erc_generation: u64,    // Already claimed as ERCs
}

pub enum MeterType { Solar, Wind, Battery, Grid }
pub enum MeterStatus { Active, Inactive, Maintenance }
```

#### Event Emissions

```rust
#[event] pub struct RegistryInitialized { authority, timestamp }
#[event] pub struct UserRegistered { user, user_type, location, timestamp }
#[event] pub struct MeterRegistered { meter_id, owner, meter_type, timestamp }
#[event] pub struct UserStatusUpdated { user, old_status, new_status, timestamp }
#[event] pub struct MeterReadingUpdated { meter_id, owner, energy_generated, energy_consumed, timestamp }
#[event] pub struct MeterBalanceSettled { meter_id, owner, tokens_to_mint, total_settled, timestamp }
```

---

### 2. Energy Token Program
**Program ID:** `94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur`

#### Purpose
Manages the GRID utility token lifecycle: minting, transferring, and burning. Integrates with Registry for settlement-based minting.

#### Key Instructions

```rust
pub mod energy_token {
    // Initialization
    pub fn initialize(ctx) -> Result<()>
    pub fn initialize_token(ctx) -> Result<()>
    
    // Token Operations
    pub fn transfer_tokens(ctx, amount) -> Result<()>
    pub fn burn_tokens(ctx, amount) -> Result<()>
    
    // Minting (CPI to Registry)
    pub fn mint_grid_tokens(ctx) -> Result<()>
    
    // Governance
    pub fn add_rec_validator(ctx, validator_pubkey, authority_name) -> Result<()>
}
```

#### Account Structure

**TokenInfo** (PDA: `["token_info"]`)
```rust
pub struct TokenInfo {
    pub authority: Pubkey,      // Mint authority
    pub mint: Pubkey,           // SPL Token mint address
    pub total_supply: u64,      // Current circulating supply
    pub created_at: i64,        // Initialization timestamp
}
```

#### Minting Process (CPI Flow)

```
┌────────────────────────────────────────────────────────────┐
│                     mint_grid_tokens()                      │
├────────────────────────────────────────────────────────────┤
│                                                              │
│  Step 1: CPI to Registry                                   │
│  ┌─────────────────────────────────────────────┐           │
│  │ registry::settle_meter_balance(ctx)         │           │
│  │   ├─ Calculate: net_gen - settled_net_gen   │           │
│  │   ├─ Update: settled_net_generation         │           │
│  │   └─ Return: tokens_to_mint                 │           │
│  └─────────────────────────────────────────────┘           │
│                      │                                      │
│                      ▼                                      │
│  Step 2: SPL Token Mint                                    │
│  ┌─────────────────────────────────────────────┐           │
│  │ token::mint_to(ctx, tokens_to_mint)         │           │
│  │   └─ Mint to user_token_account             │           │
│  └─────────────────────────────────────────────┘           │
│                      │                                      │
│                      ▼                                      │
│  Step 3: Update Supply                                     │
│  ┌─────────────────────────────────────────────┐           │
│  │ token_info.total_supply += tokens_to_mint   │           │
│  │ emit!(GridTokensMinted{...})                │           │
│  └─────────────────────────────────────────────┘           │
│                                                              │
└────────────────────────────────────────────────────────────┘
```

#### Event Emissions

```rust
#[event] pub struct GridTokensMinted { meter_owner, amount, timestamp }
```

---

### 3. Oracle Program
**Program ID:** `DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE`

#### Purpose
Bridges off-chain data (AMI readings, market prices) to on-chain programs. Only API Gateway can submit data.

#### Key Instructions

```rust
pub mod oracle {
    // Initialization
    pub fn initialize(ctx, api_gateway) -> Result<()>
    
    // Data Submission (API Gateway Only)
    pub fn submit_meter_reading(ctx, meter_id, produced, consumed, timestamp) -> Result<()>
    pub fn trigger_market_clearing(ctx) -> Result<()>
    
    // Configuration (Admin Only)
    pub fn update_oracle_status(ctx, active) -> Result<()>
    pub fn update_api_gateway(ctx, new_gateway) -> Result<()>
}
```

#### Account Structure

**OracleData** (PDA: `["oracle_data"]`)
```rust
pub struct OracleData {
    pub authority: Pubkey,              // Admin authority
    pub api_gateway: Pubkey,            // Authorized data submitter
    pub total_readings: u64,            // Total submitted readings
    pub last_reading_timestamp: i64,    // Last reading timestamp
    pub last_clearing: i64,             // Last market clearing
    pub active: bool,                   // Oracle operational status
    pub created_at: i64,                // Initialization timestamp
}
```

#### Security Model

```
┌─────────────────────────────────────────────┐
│           Oracle Security Layers             │
├─────────────────────────────────────────────┤
│                                               │
│  Layer 1: Authority Validation               │
│  ┌─────────────────────────────────────┐    │
│  │ require!(signer == api_gateway)     │    │
│  └─────────────────────────────────────┘    │
│                 │                            │
│  Layer 2: Status Check                      │
│  ┌─────────────────────────────────────┐    │
│  │ require!(oracle_data.active)        │    │
│  └─────────────────────────────────────┘    │
│                 │                            │
│  Layer 3: Data Encoding                     │
│  ┌─────────────────────────────────────┐    │
│  │ base64::encode(reading_data)        │    │
│  │ msg!("data: {}", encoded)           │    │
│  └─────────────────────────────────────┘    │
│                 │                            │
│  Layer 4: Event Emission                    │
│  ┌─────────────────────────────────────┐    │
│  │ emit!(MeterReadingSubmitted{...})   │    │
│  └─────────────────────────────────────┘    │
│                                               │
└─────────────────────────────────────────────┘
```

#### Event Emissions

```rust
#[event] pub struct MeterReadingSubmitted { meter_id, energy_produced, energy_consumed, timestamp, submitter }
#[event] pub struct MarketClearingTriggered { authority, timestamp }
#[event] pub struct OracleStatusUpdated { authority, active, timestamp }
#[event] pub struct ApiGatewayUpdated { authority, old_gateway, new_gateway, timestamp }
```

---

### 4. Trading Program
**Program ID:** `GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk`

#### Purpose
Implements the P2P energy marketplace with order book, matching engine, and settlement.

#### Key Instructions

```rust
pub mod trading {
    // Initialization
    pub fn initialize(ctx) -> Result<()>
    pub fn initialize_market(ctx) -> Result<()>
    
    // Order Management
    pub fn create_sell_order(ctx, energy_amount, price_per_kwh) -> Result<()>
    pub fn create_buy_order(ctx, energy_amount, max_price_per_kwh) -> Result<()>
    pub fn cancel_order(ctx, order_id) -> Result<()>
    
    // Matching & Settlement
    pub fn match_orders(ctx) -> Result<()>
    
    // Configuration (Admin Only)
    pub fn update_market_params(ctx, market_fee_bps, clearing_enabled) -> Result<()>
}
```

#### Account Structure

**Market** (PDA: `["market"]`)
```rust
pub struct Market {
    pub authority: Pubkey,          // Market admin
    pub active_orders: u64,         // Current open orders
    pub total_volume: u64,          // Cumulative traded volume
    pub total_trades: u64,          // Total completed trades
    pub created_at: i64,            // Market creation timestamp
    pub clearing_enabled: bool,     // Auto-clearing enabled
    pub market_fee_bps: u16,        // Fee in basis points (25 = 0.25%)
}
```

**Order** (PDA: `["order", order_id]`)
```rust
pub struct Order {
    pub seller: Pubkey,             // Seller wallet (if sell order)
    pub buyer: Pubkey,              // Buyer wallet (if buy order)
    pub amount: u64,                // Total energy amount (Wh)
    pub filled_amount: u64,         // Amount already matched
    pub price_per_kwh: u64,         // Price in GRID tokens
    pub order_type: OrderType,      // Sell or Buy
    pub status: OrderStatus,        // Active/PartiallyFilled/Completed/Cancelled/Expired
    pub created_at: i64,            // Order creation timestamp
    pub expires_at: i64,            // Expiration timestamp
}

pub enum OrderType { Sell, Buy }
pub enum OrderStatus { Active, PartiallyFilled, Completed, Cancelled, Expired }
```

**TradeRecord** (PDA: `["trade", trade_id]`)
```rust
pub struct TradeRecord {
    pub sell_order: Pubkey,         // Sell order PDA
    pub buy_order: Pubkey,          // Buy order PDA
    pub seller: Pubkey,             // Seller wallet
    pub buyer: Pubkey,              // Buyer wallet
    pub amount: u64,                // Energy traded (Wh)
    pub price_per_kwh: u64,         // Agreed price
    pub total_value: u64,           // Total GRID tokens
    pub fee_amount: u64,            // Platform fee
    pub executed_at: i64,           // Trade execution timestamp
}
```

#### Event Emissions

```rust
#[event] pub struct MarketInitialized { authority, timestamp }
#[event] pub struct SellOrderCreated { seller, order_id, amount, price_per_kwh, timestamp }
#[event] pub struct BuyOrderCreated { buyer, order_id, amount, price_per_kwh, timestamp }
#[event] pub struct OrderMatched { sell_order, buy_order, seller, buyer, amount, price, total_value, fee_amount, timestamp }
#[event] pub struct OrderCancelled { order_id, user, timestamp }
#[event] pub struct MarketParamsUpdated { authority, market_fee_bps, clearing_enabled, timestamp }
```

---

### 5. Governance Program
**Program ID:** `4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe`

#### Purpose
Implements Proof-of-Authority (PoA) governance for REC certification and system-wide controls.

#### Key Instructions

```rust
pub mod governance {
    // Initialization
    pub fn initialize_poa(ctx) -> Result<()>
    
    // Emergency Controls (REC Authority)
    pub fn emergency_pause(ctx) -> Result<()>
    pub fn emergency_unpause(ctx) -> Result<()>
    
    // ERC Certification (REC Authority)
    pub fn issue_erc(ctx, certificate_id, energy_amount, renewable_source, validation_data) -> Result<()>
    pub fn validate_erc_for_trading(ctx) -> Result<()>
    
    // Configuration (Engineering Department)
    pub fn update_governance_config(ctx, erc_validation_enabled) -> Result<()>
    pub fn set_maintenance_mode(ctx, maintenance_enabled) -> Result<()>
    pub fn update_erc_limits(ctx, min_energy, max_erc, validity_period) -> Result<()>
    pub fn update_authority_info(ctx, contact_info) -> Result<()>
    
    // Statistics
    pub fn get_governance_stats(ctx) -> Result<GovernanceStats>
}
```

#### Account Structure

**PoAConfig** (PDA: `["poa_config"]`)
```rust
pub struct PoAConfig {
    // Authority Configuration
    pub authority: Pubkey,                      // REC certifying entity
    pub authority_name: String,                 // "REC" (max 64 chars)
    pub contact_info: String,                   // Contact details (max 128 chars)
    pub version: u8,                            // Config version
    
    // Emergency Controls
    pub emergency_paused: bool,                 // System-wide pause
    pub emergency_timestamp: Option<i64>,       // When paused
    pub emergency_reason: Option<String>,       // Pause reason (max 128 chars)
    pub maintenance_mode: bool,                 // Maintenance status
    
    // ERC Certificate Configuration
    pub erc_validation_enabled: bool,           // ERC validation active
    pub min_energy_amount: u64,                 // Minimum kWh for ERC
    pub max_erc_amount: u64,                    // Maximum kWh per ERC
    pub erc_validity_period: i64,               // Validity in seconds
    pub auto_revoke_expired: bool,              // Auto-revoke on expiry
    pub require_oracle_validation: bool,        // Require oracle check
    
    // Advanced Features
    pub delegation_enabled: bool,               // Allow ERC delegation
    pub oracle_authority: Option<Pubkey>,       // Oracle for validation
    pub min_oracle_confidence: u8,              // Min confidence (0-100)
    pub allow_certificate_transfers: bool,      // Transfer ERCs
    
    // Statistics
    pub total_ercs_issued: u64,                 // Total ERCs issued
    pub total_ercs_validated: u64,              // Total validated
    pub total_ercs_revoked: u64,                // Total revoked
    pub total_energy_certified: u64,            // Total energy (kWh)
    
    // Timestamps
    pub created_at: i64,                        // Initialization
    pub last_updated: i64,                      // Last config update
    pub last_erc_issued_at: Option<i64>,        // Last ERC issuance
}
```

**ErcCertificate** (PDA: `["erc_certificate", certificate_id]`)
```rust
pub struct ErcCertificate {
    pub certificate_id: String,                 // Unique ID (max 64 chars)
    pub authority: Pubkey,                      // Issuing authority
    pub energy_amount: u64,                     // Renewable energy (kWh)
    pub renewable_source: String,               // Solar/Wind/etc (max 64 chars)
    pub validation_data: String,                // Additional data (max 256 chars)
    pub issued_at: i64,                         // Issuance timestamp
    pub expires_at: Option<i64>,                // Expiration timestamp
    pub status: ErcStatus,                      // Valid/Expired/Revoked/Pending
    pub validated_for_trading: bool,            // Trading validation
    pub trading_validated_at: Option<i64>,      // Trading validation timestamp
}

pub enum ErcStatus { Valid, Expired, Revoked, Pending }
```

**MeterAccount** (Referenced from Registry)
```rust
pub struct MeterAccount {
    // ... (see Registry section)
    pub claimed_erc_generation: u64,    // Double-claim prevention
}
```

#### Event Emissions

```rust
#[event] pub struct PoAInitialized { authority, authority_name, timestamp }
#[event] pub struct EmergencyPauseActivated { authority, reason, timestamp }
#[event] pub struct EmergencyPauseDeactivated { authority, timestamp }
#[event] pub struct ErcIssued { certificate_id, authority, energy_amount, renewable_source, timestamp }
#[event] pub struct ErcValidatedForTrading { certificate_id, authority, timestamp }
#[event] pub struct ErcRevoked { certificate_id, authority, reason, timestamp }
#[event] pub struct GovernanceConfigUpdated { authority, field, old_value, new_value, timestamp }
#[event] pub struct MaintenanceModeChanged { authority, enabled, timestamp }
```

---

## Data Flow

### User Registration Flow

```
┌─────────────────────────────────────────────────────────────┐
│                   User Registration                          │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  1. User → API Gateway                                       │
│     POST /api/users/register                                 │
│     { user_type, location, wallet_pubkey }                   │
│                                                               │
│  2. API Gateway → Registry Program                           │
│     registry::register_user(ctx, user_type, location)        │
│                                                               │
│  3. Registry Program Actions:                                │
│     ┌──────────────────────────────────────────┐            │
│     │ a. Create UserAccount PDA                │            │
│     │    seeds: ["user", user_authority]       │            │
│     │                                            │            │
│     │ b. Initialize UserAccount:                │            │
│     │    - authority = signer.key()             │            │
│     │    - user_type = Prosumer/Consumer        │            │
│     │    - location = provided_location         │            │
│     │    - status = Active                      │            │
│     │    - registered_at = current_timestamp    │            │
│     │    - meter_count = 0                      │            │
│     │                                            │            │
│     │ c. Update Registry:                       │            │
│     │    - registry.user_count += 1             │            │
│     │                                            │            │
│     │ d. Emit Event:                            │            │
│     │    UserRegistered { ... }                 │            │
│     └──────────────────────────────────────────┘            │
│                                                               │
│  4. Registry Program → API Gateway                           │
│     Return: user_account_pubkey                              │
│                                                               │
│  5. API Gateway → User                                       │
│     Response: { success, user_account }                      │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Energy Generation & Tokenization Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│             Energy Generation → GRID Token Minting                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Phase 1: Data Collection                                            │
│  ────────────────────────────────────────────────                    │
│  1. Smart Meter → AMI System                                        │
│     Reports: generation=5000Wh, consumption=2000Wh                   │
│                                                                       │
│  2. AMI → API Gateway                                                │
│     POST /api/oracle/meter-reading                                   │
│     { meter_id, generation, consumption, timestamp }                 │
│                                                                       │
│  3. API Gateway → Oracle Program                                     │
│     oracle::submit_meter_reading(ctx, meter_id, 5000, 2000, ts)     │
│                                                                       │
│  4. Oracle Program → Registry Program (CPI)                          │
│     registry::update_meter_reading(ctx, 5000, 2000, ts)             │
│                                                                       │
│  Phase 2: Balance Settlement                                         │
│  ────────────────────────────────────────────────                    │
│  5. Registry Program Actions:                                        │
│     ┌────────────────────────────────────────────┐                  │
│     │ meter.total_generation = 5000              │                  │
│     │ meter.total_consumption = 2000             │                  │
│     │ meter.last_reading_at = timestamp          │                  │
│     │                                             │                  │
│     │ emit!(MeterReadingUpdated { ... })         │                  │
│     └────────────────────────────────────────────┘                  │
│                                                                       │
│  Phase 3: Token Minting (User-Initiated)                            │
│  ────────────────────────────────────────────────                    │
│  6. User → API Gateway                                               │
│     POST /api/tokens/mint                                            │
│     { meter_id, meter_owner_signature }                              │
│                                                                       │
│  7. API Gateway → Energy Token Program                               │
│     energy_token::mint_grid_tokens(ctx)                              │
│                                                                       │
│  8. Energy Token Program → Registry Program (CPI)                    │
│     tokens = registry::settle_meter_balance(ctx)                     │
│                                                                       │
│  9. Registry Program Calculation:                                    │
│     ┌────────────────────────────────────────────┐                  │
│     │ current_net = generation - consumption     │                  │
│     │            = 5000 - 2000 = 3000Wh         │                  │
│     │                                             │                  │
│     │ already_settled = meter.settled_net_gen    │                  │
│     │                 = 0Wh (first time)         │                  │
│     │                                             │                  │
│     │ tokens_to_mint = current_net - settled     │                  │
│     │                = 3000 - 0 = 3000 tokens   │                  │
│     │                                             │                  │
│     │ meter.settled_net_generation = 3000        │                  │
│     │                                             │                  │
│     │ emit!(MeterBalanceSettled { ... })         │                  │
│     │ return tokens_to_mint                      │                  │
│     └────────────────────────────────────────────┘                  │
│                                                                       │
│  10. Energy Token Program → SPL Token Program (CPI)                  │
│      token::mint_to(ctx, 3000)                                       │
│                                                                       │
│  11. Energy Token Program Actions:                                   │
│      ┌────────────────────────────────────────────┐                 │
│      │ Mint 3000 tokens to user_token_account     │                 │
│      │ token_info.total_supply += 3000            │                 │
│      │ emit!(GridTokensMinted { ... })            │                 │
│      └────────────────────────────────────────────┘                 │
│                                                                       │
│  12. API Gateway → User                                              │
│      Response: { success, minted_amount: 3000 }                      │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

### P2P Trading Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                  P2P Energy Trading Workflow                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Step 1: Sell Order Creation                                        │
│  ────────────────────────────────────────────────                    │
│  Prosumer A → Trading Program                                        │
│  trading::create_sell_order(ctx, 2000Wh, 0.15 GRID/Wh)              │
│                                                                       │
│  Actions:                                                             │
│  ┌───────────────────────────────────────────────┐                  │
│  │ • Create Order PDA                            │                  │
│  │ • Lock 2000 GRID tokens in escrow             │                  │
│  │ • market.active_orders += 1                   │                  │
│  │ • emit!(SellOrderCreated { ... })             │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Step 2: Buy Order Creation                                         │
│  ────────────────────────────────────────────────                    │
│  Consumer B → Trading Program                                        │
│  trading::create_buy_order(ctx, 1500Wh, 0.16 GRID/Wh)               │
│                                                                       │
│  Actions:                                                             │
│  ┌───────────────────────────────────────────────┐                  │
│  │ • Create Order PDA                            │                  │
│  │ • Lock 240 GRID tokens in escrow              │                  │
│  │   (1500Wh * 0.16 = 240 GRID)                  │                  │
│  │ • market.active_orders += 1                   │                  │
│  │ • emit!(BuyOrderCreated { ... })              │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Step 3: Order Matching (Auto or Manual)                            │
│  ────────────────────────────────────────────────                    │
│  Matching Engine → Trading Program                                   │
│  trading::match_orders(ctx, sell_order_pda, buy_order_pda)          │
│                                                                       │
│  Matching Logic:                                                      │
│  ┌───────────────────────────────────────────────┐                  │
│  │ if buy_price >= sell_price:                   │                  │
│  │   match_amount = min(sell_amount, buy_amount) │                  │
│  │                = min(2000, 1500) = 1500Wh     │                  │
│  │                                                │                  │
│  │   agreed_price = sell_price = 0.15 GRID/Wh   │                  │
│  │   total_value = 1500 * 0.15 = 225 GRID        │                  │
│  │   platform_fee = 225 * 0.0025 = 0.5625 GRID   │                  │
│  │   seller_receives = 225 - 0.5625 = 224.44 GRID│                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Step 4: Settlement                                                  │
│  ────────────────────────────────────────────────                    │
│  Actions:                                                             │
│  ┌───────────────────────────────────────────────┐                  │
│  │ • Transfer 224.44 GRID to Prosumer A          │                  │
│  │ • Transfer 0.5625 GRID to platform            │                  │
│  │ • Transfer 1500Wh energy rights to Consumer B │                  │
│  │ • Return 15 GRID excess to Consumer B         │                  │
│  │   (240 locked - 225 used = 15)                │                  │
│  │                                                │                  │
│  │ • Update sell_order:                          │                  │
│  │   filled_amount = 1500Wh                      │                  │
│  │   remaining = 500Wh                           │                  │
│  │   status = PartiallyFilled                    │                  │
│  │                                                │                  │
│  │ • Update buy_order:                           │                  │
│  │   filled_amount = 1500Wh                      │                  │
│  │   status = Completed                          │                  │
│  │                                                │                  │
│  │ • Create TradeRecord PDA                      │                  │
│  │ • market.total_volume += 225 GRID             │                  │
│  │ • market.total_trades += 1                    │                  │
│  │ • market.active_orders -= 1 (buy completed)   │                  │
│  │                                                │                  │
│  │ • emit!(OrderMatched { ... })                 │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

### ERC Certification Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│              ERC (Renewable Energy Certificate) Flow                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Prerequisite: Prosumer has generated renewable energy               │
│  ────────────────────────────────────────────────                    │
│  meter.total_generation = 10,000Wh (solar)                           │
│  meter.claimed_erc_generation = 0Wh                                  │
│                                                                       │
│  Step 1: ERC Issuance Request                                        │
│  ────────────────────────────────────────────────                    │
│  Prosumer → API Gateway                                              │
│  POST /api/governance/erc/issue                                      │
│  {                                                                    │
│    meter_id: "METER-001",                                            │
│    energy_amount: 5000Wh,                                            │
│    renewable_source: "Solar",                                        │
│    validation_data: "REC-CERT-2025-001"                              │
│  }                                                                    │
│                                                                       │
│  Step 2: REC Authority Validation                                    │
│  ────────────────────────────────────────────────                    │
│  API Gateway → Governance Program                                    │
│  governance::issue_erc(ctx, ...)                                     │
│                                                                       │
│  Validation Checks:                                                   │
│  ┌───────────────────────────────────────────────┐                  │
│  │ 1. Check poa_config.is_operational()          │                  │
│  │    - Not emergency_paused                     │                  │
│  │    - Not maintenance_mode                     │                  │
│  │                                                │                  │
│  │ 2. Check poa_config.can_issue_erc()           │                  │
│  │    - erc_validation_enabled = true            │                  │
│  │                                                │                  │
│  │ 3. Check signer = poa_config.authority        │                  │
│  │    - Only REC authority can issue             │                  │
│  │                                                │                  │
│  │ 4. Validate energy amount:                    │                  │
│  │    - energy_amount >= min_energy_amount       │                  │
│  │    - energy_amount <= max_erc_amount          │                  │
│  │                                                │                  │
│  │ 5. Check double-claim prevention:             │                  │
│  │    available = total_gen - claimed_erc_gen    │                  │
│  │              = 10,000 - 0 = 10,000Wh          │                  │
│  │    require!(energy_amount <= available)       │                  │
│  │    require!(5000 <= 10,000) ✓                 │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Step 3: ERC Creation                                                │
│  ────────────────────────────────────────────────                    │
│  Actions:                                                             │
│  ┌───────────────────────────────────────────────┐                  │
│  │ • Generate certificate_id                     │                  │
│  │   "ERC-2025-11-07-001"                        │                  │
│  │                                                │                  │
│  │ • Create ErcCertificate PDA:                  │                  │
│  │   seeds: ["erc_certificate", certificate_id]  │                  │
│  │                                                │                  │
│  │ • Initialize ErcCertificate:                  │                  │
│  │   - certificate_id = "ERC-2025-11-07-001"     │                  │
│  │   - authority = REC_authority_pubkey          │                  │
│  │   - energy_amount = 5000Wh                    │                  │
│  │   - renewable_source = "Solar"                │                  │
│  │   - validation_data = "REC-CERT-2025-001"     │                  │
│  │   - issued_at = current_timestamp             │                  │
│  │   - expires_at = issued_at + validity_period  │                  │
│  │   - status = Valid                            │                  │
│  │   - validated_for_trading = false             │                  │
│  │                                                │                  │
│  │ • Update meter_account:                       │                  │
│  │   meter.claimed_erc_generation += 5000        │                  │
│  │   (Now: claimed = 5000Wh)                     │                  │
│  │                                                │                  │
│  │ • Update poa_config statistics:               │                  │
│  │   - total_ercs_issued += 1                    │                  │
│  │   - total_energy_certified += 5000            │                  │
│  │   - last_erc_issued_at = current_timestamp    │                  │
│  │                                                │                  │
│  │ • emit!(ErcIssued { ... })                    │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Step 4: Trading Validation (When needed)                            │
│  ────────────────────────────────────────────────                    │
│  Prosumer → Governance Program                                       │
│  governance::validate_erc_for_trading(ctx)                           │
│                                                                       │
│  Actions:                                                             │
│  ┌───────────────────────────────────────────────┐                  │
│  │ • Verify ERC status = Valid                   │                  │
│  │ • Check not expired                           │                  │
│  │ • Set validated_for_trading = true            │                  │
│  │ • Set trading_validated_at = timestamp        │                  │
│  │ • poa_config.total_ercs_validated += 1        │                  │
│  │ • emit!(ErcValidatedForTrading { ... })       │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
│  Double-Claim Prevention Example:                                    │
│  ────────────────────────────────────────────────                    │
│  Meter Status After ERC Issuance:                                    │
│  ┌───────────────────────────────────────────────┐                  │
│  │ total_generation = 10,000Wh                   │                  │
│  │ claimed_erc_generation = 5,000Wh              │                  │
│  │ available_for_erc = 5,000Wh remaining         │                  │
│  │                                                │                  │
│  │ If prosumer tries to claim 6,000Wh again:     │                  │
│  │ ❌ FAIL: 6,000 > available (5,000)            │                  │
│  │                                                │                  │
│  │ If prosumer generates 5,000Wh more:           │                  │
│  │ total_generation = 15,000Wh                   │                  │
│  │ claimed_erc_generation = 5,000Wh              │                  │
│  │ available_for_erc = 10,000Wh ✓                │                  │
│  └───────────────────────────────────────────────┘                  │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Security Model

### Authority Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│                  Authority Hierarchy                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Level 1: System Authority                                   │
│  ┌──────────────────────────────────────────┐               │
│  │ Program Deployer (Initial Authority)     │               │
│  │ • Can upgrade programs                   │               │
│  │ • Can initialize programs                │               │
│  │ • Can transfer authority                 │               │
│  └──────────────────────────────────────────┘               │
│                │                                             │
│                ├─────────────────────────────────┐           │
│                │                                 │           │
│  Level 2a: REC Authority                Level 2b: API Gateway│
│  ┌──────────────────────────┐         ┌────────────────────┐│
│  │ Governance Program       │         │ Oracle Program     ││
│  │ • Issue ERCs             │         │ • Submit readings  ││
│  │ • Validate ERCs          │         │ • Trigger clearing ││
│  │ • Emergency pause        │         └────────────────────┘│
│  │ • Emergency unpause      │                               │
│  └──────────────────────────┘                               │
│                                                               │
│  Level 3: Program Authorities                                │
│  ┌──────────────────────────────────────────┐               │
│  │ Registry Authority                       │               │
│  │ • Update user status                     │               │
│  │ • Admin operations                       │               │
│  │                                            │               │
│  │ Token Authority                           │               │
│  │ • Add REC validators                      │               │
│  │ • Token initialization                    │               │
│  │                                            │               │
│  │ Trading Authority                         │               │
│  │ • Update market params                    │               │
│  │ • Enable/disable clearing                 │               │
│  └──────────────────────────────────────────┘               │
│                                                               │
│  Level 4: Users                                              │
│  ┌──────────────────────────────────────────┐               │
│  │ Prosumers & Consumers                    │               │
│  │ • Manage own accounts                    │               │
│  │ • Own token operations                   │               │
│  │ • Create/cancel orders                   │               │
│  └──────────────────────────────────────────┘               │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### PDA Security

All program state is stored in Program Derived Addresses (PDAs) which are deterministic and cannot be created by external users:

```rust
// Registry PDAs
Registry:      seeds = [b"registry"]
UserAccount:   seeds = [b"user", user_authority.key()]
MeterAccount:  seeds = [b"meter", meter_id.as_bytes()]

// Energy Token PDAs
TokenInfo:     seeds = [b"token_info"]

// Oracle PDAs
OracleData:    seeds = [b"oracle_data"]

// Trading PDAs
Market:        seeds = [b"market"]
Order:         seeds = [b"order", order_id]
TradeRecord:   seeds = [b"trade", trade_id]

// Governance PDAs
PoAConfig:         seeds = [b"poa_config"]
ErcCertificate:    seeds = [b"erc_certificate", certificate_id.as_bytes()]
```

### Cross-Program Invocation (CPI) Security

```
┌─────────────────────────────────────────────────────────────┐
│                    CPI Security Model                        │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Energy Token → Registry (Settlement)                        │
│  ┌──────────────────────────────────────────┐               │
│  │ 1. Verify meter_owner is signer          │               │
│  │ 2. Call registry::settle_meter_balance   │               │
│  │ 3. Registry verifies meter ownership     │               │
│  │ 4. Registry returns tokens_to_mint       │               │
│  │ 5. Energy Token validates return value   │               │
│  │ 6. Mint tokens using PDA authority       │               │
│  └──────────────────────────────────────────┘               │
│                                                               │
│  Oracle → Registry (Meter Update)                            │
│  ┌──────────────────────────────────────────┐               │
│  │ 1. Verify signer = api_gateway           │               │
│  │ 2. Verify oracle_data.active = true      │               │
│  │ 3. Call registry::update_meter_reading   │               │
│  │ 4. Registry validates meter exists       │               │
│  │ 5. Registry updates meter data           │               │
│  └──────────────────────────────────────────┘               │
│                                                               │
│  Governance → Registry (ERC Issuance)                        │
│  ┌──────────────────────────────────────────┐               │
│  │ 1. Verify signer = REC authority         │               │
│  │ 2. Verify poa_config.is_operational()    │               │
│  │ 3. Load meter_account from registry      │               │
│  │ 4. Check double-claim prevention         │               │
│  │ 5. Update meter.claimed_erc_generation   │               │
│  │ 6. Create ErcCertificate PDA             │               │
│  └──────────────────────────────────────────┘               │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Integration Patterns

### API Gateway ↔ Blockchain

```typescript
// API Gateway integration pattern
class SolanaClient {
  private connection: Connection;
  private program: Program<EnergyToken>;
  
  async mintGridTokens(
    meterOwner: Keypair,
    meterAccount: PublicKey
  ): Promise<TransactionSignature> {
    const tx = await this.program.methods
      .mintGridTokens()
      .accounts({
        tokenInfo: this.getTokenInfoPDA(),
        mint: this.mintAddress,
        meterAccount: meterAccount,
        userTokenAccount: this.getUserTokenAccount(meterOwner.publicKey),
        meterOwner: meterOwner.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        registryProgram: REGISTRY_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([meterOwner])
      .rpc();
    
    return tx;
  }
}
```

### Event Listening

```typescript
// Event listener pattern
class BlockchainEventListener {
  async listenToMintEvents() {
    this.program.addEventListener('GridTokensMinted', (event, slot) => {
      console.log(`Minted ${event.amount} tokens to ${event.meterOwner}`);
      
      // Update database
      await this.db.tokens.create({
        user: event.meterOwner.toString(),
        amount: event.amount.toString(),
        timestamp: event.timestamp.toString(),
        transactionSlot: slot,
      });
      
      // Notify user
      await this.notificationService.send(
        event.meterOwner,
        `Successfully minted ${event.amount} GRID tokens`
      );
    });
  }
}
```

---

## Deployment

### Program IDs (Localnet)

```toml
[programs.localnet]
energy_token = "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur"
governance = "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe"
oracle = "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE"
registry = "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7"
trading = "GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk"
```

### Deployment Commands

```bash
# Build all programs
cd anchor
anchor build

# Deploy to localnet
anchor deploy

# Run integration tests
anchor test

# Deploy to devnet
anchor deploy --provider.cluster devnet

# Upgrade program (maintain state)
anchor upgrade <program-path> --program-id <program-id>
```

### Initialization Sequence

```bash
# 1. Initialize Registry
anchor run initialize-registry

# 2. Initialize Oracle with API Gateway
anchor run initialize-oracle --api-gateway <PUBKEY>

# 3. Initialize Energy Token
anchor run initialize-token

# 4. Initialize Trading Market
anchor run initialize-market

# 5. Initialize Governance (PoA)
anchor run initialize-poa --authority <REC_PUBKEY>
```

---

## Appendix

### Account Size Calculations

| Program | Account Type | Size (bytes) | Rent (SOL/year) |
|---------|--------------|--------------|-----------------|
| Registry | Registry | 56 | ~0.0004 |
| Registry | UserAccount | 185 | ~0.0013 |
| Registry | MeterAccount | 241 | ~0.0017 |
| Energy Token | TokenInfo | 72 | ~0.0005 |
| Oracle | OracleData | 105 | ~0.0007 |
| Trading | Market | 89 | ~0.0006 |
| Trading | Order | 137 | ~0.0010 |
| Trading | TradeRecord | 145 | ~0.0010 |
| Governance | PoAConfig | 474 | ~0.0033 |
| Governance | ErcCertificate | 459 | ~0.0032 |

### Error Codes

```rust
// Registry Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized user")] UnauthorizedUser = 6000,
    #[msg("Unauthorized authority")] UnauthorizedAuthority = 6001,
    #[msg("Invalid user status")] InvalidUserStatus = 6002,
    #[msg("Invalid meter status")] InvalidMeterStatus = 6003,
    #[msg("User not found")] UserNotFound = 6004,
    #[msg("Meter not found")] MeterNotFound = 6005,
    #[msg("No unsettled balance to tokenize")] NoUnsettledBalance = 6006,
}

// Energy Token Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized authority")] UnauthorizedAuthority = 6100,
    #[msg("Invalid meter")] InvalidMeter = 6101,
    #[msg("Insufficient token balance")] InsufficientBalance = 6102,
}

// Oracle Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized authority")] UnauthorizedAuthority = 6200,
    #[msg("Unauthorized API Gateway")] UnauthorizedGateway = 6201,
    #[msg("Oracle is inactive")] OracleInactive = 6202,
    #[msg("Invalid meter reading")] InvalidMeterReading = 6203,
    #[msg("Market clearing in progress")] MarketClearingInProgress = 6204,
}

// Trading Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized authority")] UnauthorizedAuthority = 6300,
    #[msg("Invalid amount")] InvalidAmount = 6301,
    #[msg("Invalid price")] InvalidPrice = 6302,
    #[msg("Inactive sell order")] InactiveSellOrder = 6303,
    #[msg("Inactive buy order")] InactiveBuyOrder = 6304,
    #[msg("Price mismatch")] PriceMismatch = 6305,
    #[msg("Order not cancellable")] OrderNotCancellable = 6306,
    #[msg("Insufficient escrow balance")] InsufficientEscrowBalance = 6307,
}

// Governance Errors
#[error_code]
pub enum GovernanceError {
    #[msg("Unauthorized authority")] UnauthorizedAuthority = 6400,
    #[msg("System is paused")] SystemPaused = 6401,
    #[msg("System is in maintenance")] MaintenanceMode = 6402,
    #[msg("ERC validation disabled")] ErcValidationDisabled = 6403,
    #[msg("Invalid minimum energy")] InvalidMinimumEnergy = 6404,
    #[msg("Invalid maximum energy")] InvalidMaximumEnergy = 6405,
    #[msg("Invalid validity period")] InvalidValidityPeriod = 6406,
    #[msg("Invalid oracle confidence")] InvalidOracleConfidence = 6407,
    #[msg("Insufficient available energy")] InsufficientAvailableEnergy = 6408,
    #[msg("Certificate expired")] CertificateExpired = 6409,
    #[msg("Certificate revoked")] CertificateRevoked = 6410,
}
```

---

**End of Document**
