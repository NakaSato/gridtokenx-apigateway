# Anchor Programs Quick Reference Guide

**Version:** 1.0  
**Last Updated:** November 7, 2025

## Table of Contents

1. [Program IDs](#program-ids)
2. [Quick Start Commands](#quick-start-commands)
3. [Account Structures](#account-structures)
4. [Instruction Reference](#instruction-reference)
5. [Error Codes](#error-codes)
6. [Common Patterns](#common-patterns)

---

## Program IDs

### Localnet

```
Registry:      2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7
Energy Token:  94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur
Oracle:        DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE
Trading:       GZnqNTJsre6qB4pWCQRE9FiJU2GUeBtBDPp6s7zosctk
Governance:    4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe
```

---

## Quick Start Commands

### Build & Deploy

```bash
# Build all programs
cd anchor && anchor build

# Deploy to localnet
anchor deploy

# Run tests
anchor test

# Deploy specific program
anchor deploy --program-name registry
```

### Initialize Programs

```bash
# 1. Registry
anchor run initialize-registry

# 2. Oracle (with API Gateway pubkey)
anchor run initialize-oracle --api-gateway <PUBKEY>

# 3. Energy Token
anchor run initialize-token --mint <MINT_PUBKEY>

# 4. Trading Market
anchor run initialize-market

# 5. Governance (with REC authority)
anchor run initialize-poa --authority <REC_PUBKEY>
```

### Common Operations

```bash
# Register user
anchor run register-user --user-type prosumer --location "Bangkok"

# Register meter
anchor run register-meter --meter-id "METER-001" --type solar

# Mint tokens
anchor run mint-tokens --meter-id "METER-001"

# Create sell order
anchor run create-sell-order --amount 2000 --price 0.15

# Issue ERC
anchor run issue-erc --meter-id "METER-001" --amount 5000 --source "Solar"
```

---

## Account Structures

### Registry Program

#### Registry (PDA: `["registry"]`)
- **Size:** 56 bytes
- **Fields:**
  - `authority: Pubkey` - Admin authority
  - `user_count: u64` - Total registered users
  - `meter_count: u64` - Total registered meters
  - `created_at: i64` - Initialization timestamp

#### UserAccount (PDA: `["user", authority]`)
- **Size:** 185 bytes
- **Fields:**
  - `authority: Pubkey` - Owner wallet
  - `user_type: UserType` - Prosumer/Consumer
  - `location: String` - Max 100 chars
  - `status: UserStatus` - Active/Suspended/Inactive
  - `registered_at: i64`
  - `meter_count: u32`

#### MeterAccount (PDA: `["meter", meter_id]`)
- **Size:** 241 bytes
- **Fields:**
  - `meter_id: String` - Max 50 chars
  - `owner: Pubkey`
  - `meter_type: MeterType` - Solar/Wind/Battery/Grid
  - `status: MeterStatus` - Active/Inactive/Maintenance
  - `total_generation: u64` - Cumulative energy produced (Wh)
  - `total_consumption: u64` - Cumulative energy consumed (Wh)
  - `settled_net_generation: u64` - ⚠️ Double-mint prevention
  - `claimed_erc_generation: u64` - ⚠️ Double-claim prevention

### Energy Token Program

#### TokenInfo (PDA: `["token_info"]`)
- **Size:** 72 bytes
- **Fields:**
  - `authority: Pubkey` - Mint authority
  - `mint: Pubkey` - SPL Token mint
  - `total_supply: u64`
  - `created_at: i64`

### Oracle Program

#### OracleData (PDA: `["oracle_data"]`)
- **Size:** 105 bytes
- **Fields:**
  - `authority: Pubkey` - Admin
  - `api_gateway: Pubkey` - ⚠️ Only this can submit data
  - `total_readings: u64`
  - `last_reading_timestamp: i64`
  - `last_clearing: i64`
  - `active: bool`

### Trading Program

#### Market (PDA: `["market"]`)
- **Size:** 89 bytes
- **Fields:**
  - `authority: Pubkey`
  - `active_orders: u64`
  - `total_volume: u64`
  - `total_trades: u64`
  - `clearing_enabled: bool`
  - `market_fee_bps: u16` - Default: 25 (0.25%)

#### Order (PDA: `["order", order_id]`)
- **Size:** 137 bytes
- **Fields:**
  - `seller: Pubkey`
  - `buyer: Pubkey`
  - `amount: u64` - Energy in Wh
  - `filled_amount: u64`
  - `price_per_kwh: u64`
  - `order_type: OrderType` - Sell/Buy
  - `status: OrderStatus`

#### TradeRecord (PDA: `["trade", trade_id]`)
- **Size:** 145 bytes
- **Immutable record of completed trade**

### Governance Program

#### PoAConfig (PDA: `["poa_config"]`)
- **Size:** 474 bytes
- **Fields:**
  - `authority: Pubkey` - REC authority
  - `authority_name: String` - Max 64 chars
  - `emergency_paused: bool`
  - `maintenance_mode: bool`
  - `erc_validation_enabled: bool`
  - `min_energy_amount: u64`
  - `max_erc_amount: u64`
  - `erc_validity_period: i64`
  - `total_ercs_issued: u64`
  - Statistics...

#### ErcCertificate (PDA: `["erc_certificate", cert_id]`)
- **Size:** 459 bytes
- **Fields:**
  - `certificate_id: String` - Max 64 chars
  - `authority: Pubkey`
  - `energy_amount: u64`
  - `renewable_source: String` - Max 64 chars
  - `validation_data: String` - Max 256 chars
  - `status: ErcStatus` - Valid/Expired/Revoked/Pending
  - `validated_for_trading: bool`

---

## Instruction Reference

### Registry Program

#### `initialize(ctx: Context<Initialize>) -> Result<()>`
- **Authority:** System Admin
- **Description:** Initialize the registry
- **Accounts:** `registry`, `authority`, `system_program`

#### `register_user(ctx, user_type, location) -> Result<()>`
- **Authority:** Any user (self-registration)
- **Description:** Register as Prosumer or Consumer
- **Accounts:** `registry`, `user_account`, `user_authority`, `system_program`

#### `register_meter(ctx, meter_id, meter_type) -> Result<()>`
- **Authority:** Registered user
- **Description:** Register a smart meter
- **Accounts:** `registry`, `user_account`, `meter_account`, `user_authority`, `system_program`

#### `update_meter_reading(ctx, generated, consumed, timestamp) -> Result<()>`
- **Authority:** Oracle program (via CPI)
- **Description:** Update meter energy data
- **Accounts:** `meter_account`, `oracle_authority`

#### `settle_meter_balance(ctx) -> Result<u64>`
- **Authority:** Energy Token program (via CPI)
- **Description:** Calculate and settle tokens to mint
- **Returns:** Amount of tokens to mint
- **Accounts:** `meter_account`, `meter_owner`

#### `get_unsettled_balance(ctx) -> Result<u64>`
- **Authority:** Anyone (view function)
- **Description:** Check how much energy can be tokenized
- **Returns:** Unsettled energy amount
- **Accounts:** `meter_account`

### Energy Token Program

#### `initialize_token(ctx) -> Result<()>`
- **Authority:** System Admin
- **Description:** Initialize token program
- **Accounts:** `token_info`, `mint`, `authority`, `system_program`

#### `mint_grid_tokens(ctx) -> Result<()>`
- **Authority:** Meter owner
- **Description:** Mint GRID tokens from settled energy
- **Process:**
  1. CPI to `registry::settle_meter_balance`
  2. CPI to `token::mint_to` (SPL Token)
  3. Update `total_supply`
- **Accounts:** `token_info`, `mint`, `meter_account`, `user_token_account`, `meter_owner`, `token_program`, `registry_program`, `system_program`

#### `transfer_tokens(ctx, amount) -> Result<()>`
- **Authority:** Token owner
- **Description:** Transfer tokens between accounts
- **Accounts:** `from_token_account`, `to_token_account`, `from_authority`, `token_program`

#### `burn_tokens(ctx, amount) -> Result<()>`
- **Authority:** Token owner
- **Description:** Burn tokens (energy consumption)
- **Accounts:** `token_info`, `mint`, `token_account`, `authority`, `token_program`

### Oracle Program

#### `initialize(ctx, api_gateway) -> Result<()>`
- **Authority:** System Admin
- **Description:** Initialize oracle with API Gateway pubkey
- **Accounts:** `oracle_data`, `authority`, `system_program`

#### `submit_meter_reading(ctx, meter_id, produced, consumed, timestamp) -> Result<()>`
- **Authority:** API Gateway ONLY
- **Description:** Submit AMI meter reading
- **Process:** CPI to `registry::update_meter_reading`
- **Accounts:** `oracle_data`, `authority` (must be API Gateway)

#### `trigger_market_clearing(ctx) -> Result<()>`
- **Authority:** API Gateway ONLY
- **Description:** Trigger automated market clearing
- **Accounts:** `oracle_data`, `authority`

#### `update_api_gateway(ctx, new_gateway) -> Result<()>`
- **Authority:** Oracle admin
- **Description:** Change API Gateway address
- **Accounts:** `oracle_data`, `authority`

### Trading Program

#### `initialize_market(ctx) -> Result<()>`
- **Authority:** System Admin
- **Description:** Initialize the trading market
- **Accounts:** `market`, `authority`, `system_program`

#### `create_sell_order(ctx, energy_amount, price_per_kwh) -> Result<()>`
- **Authority:** Any prosumer
- **Description:** Create sell order for energy
- **Process:**
  1. Create Order PDA
  2. Transfer tokens to escrow (via CPI)
- **Accounts:** `market`, `authority`, `system_program`

#### `create_buy_order(ctx, energy_amount, max_price_per_kwh) -> Result<()>`
- **Authority:** Any user
- **Description:** Create buy order for energy
- **Process:**
  1. Create Order PDA
  2. Lock tokens in escrow
- **Accounts:** `market`, `authority`, `system_program`

#### `match_orders(ctx) -> Result<()>`
- **Authority:** Matching engine or admin
- **Description:** Match and settle buy/sell orders
- **Process:**
  1. Validate price compatibility
  2. Transfer tokens (via CPI)
  3. Create TradeRecord
  4. Update order states
- **Accounts:** `market`, `authority`

#### `cancel_order(ctx, order_id) -> Result<()>`
- **Authority:** Order creator
- **Description:** Cancel active order
- **Process:** Return escrowed tokens
- **Accounts:** `market`, `authority`

### Governance Program

#### `initialize_poa(ctx) -> Result<()>`
- **Authority:** System Admin
- **Description:** Initialize Proof-of-Authority governance
- **Accounts:** `poa_config`, `authority`, `system_program`

#### `issue_erc(ctx, certificate_id, energy_amount, renewable_source, validation_data) -> Result<()>`
- **Authority:** REC Authority ONLY
- **Description:** Issue Energy Renewable Certificate
- **Process:**
  1. Validate system operational
  2. Check energy amount limits
  3. Verify no double-claim (via `claimed_erc_generation`)
  4. Create ErcCertificate PDA
  5. Update meter's `claimed_erc_generation`
- **Accounts:** `poa_config`, `erc_certificate`, `meter_account`, `authority`, `system_program`

#### `validate_erc_for_trading(ctx) -> Result<()>`
- **Authority:** REC Authority ONLY
- **Description:** Validate ERC for trading
- **Accounts:** `poa_config`, `erc_certificate`, `authority`

#### `emergency_pause(ctx) -> Result<()>`
- **Authority:** REC Authority ONLY
- **Description:** Emergency system pause
- **Accounts:** `poa_config`, `authority`

#### `emergency_unpause(ctx) -> Result<()>`
- **Authority:** REC Authority ONLY
- **Description:** Resume system operations
- **Accounts:** `poa_config`, `authority`

---

## Error Codes

### Registry (6000-6099)

```rust
6000  UnauthorizedUser           // Signer is not the account owner
6001  UnauthorizedAuthority      // Signer is not the registry authority
6002  InvalidUserStatus          // Invalid user status value
6003  InvalidMeterStatus         // Invalid meter status value
6004  UserNotFound               // User account doesn't exist
6005  MeterNotFound              // Meter account doesn't exist
6006  NoUnsettledBalance         // No energy to tokenize
```

### Energy Token (6100-6199)

```rust
6100  UnauthorizedAuthority      // Signer is not token authority
6101  InvalidMeter               // Meter validation failed
6102  InsufficientBalance        // Not enough tokens
```

### Oracle (6200-6299)

```rust
6200  UnauthorizedAuthority      // Signer is not oracle authority
6201  UnauthorizedGateway        // Signer is not API Gateway
6202  OracleInactive             // Oracle is disabled
6203  InvalidMeterReading        // Reading validation failed
6204  MarketClearingInProgress   // Cannot submit during clearing
```

### Trading (6300-6399)

```rust
6300  UnauthorizedAuthority      // Signer is not market authority
6301  InvalidAmount              // Invalid energy amount
6302  InvalidPrice               // Invalid price value
6303  InactiveSellOrder          // Sell order not active
6304  InactiveBuyOrder           // Buy order not active
6305  PriceMismatch              // Buy price < sell price
6306  OrderNotCancellable        // Cannot cancel order
6307  InsufficientEscrowBalance  // Not enough escrowed tokens
```

### Governance (6400-6499)

```rust
6400  UnauthorizedAuthority      // Signer is not REC authority
6401  SystemPaused               // System in emergency pause
6402  MaintenanceMode            // System in maintenance
6403  ErcValidationDisabled      // ERC validation turned off
6404  InvalidMinimumEnergy       // min_energy_amount invalid
6405  InvalidMaximumEnergy       // max_erc_amount invalid
6406  InvalidValidityPeriod      // erc_validity_period invalid
6407  InvalidOracleConfidence    // min_oracle_confidence invalid
6408  InsufficientAvailableEnergy // Not enough unclaimed energy
6409  CertificateExpired         // ERC has expired
6410  CertificateRevoked         // ERC has been revoked
```

---

## Common Patterns

### 1. Double-Spend Prevention

#### GRID Token Minting
```rust
// In Registry::settle_meter_balance()
let current_net_generation = meter.total_generation - meter.total_consumption;
let tokens_to_mint = current_net_generation - meter.settled_net_generation;

require!(tokens_to_mint > 0, ErrorCode::NoUnsettledBalance);

// Update tracker to prevent double-mint
meter.settled_net_generation = current_net_generation;
```

**Key Point:** `settled_net_generation` tracks what's already been minted.

#### ERC Certification
```rust
// In Governance::issue_erc()
let available_for_erc = meter.total_generation - meter.claimed_erc_generation;

require!(
    energy_amount <= available_for_erc,
    ErrorCode::InsufficientAvailableEnergy
);

// Update tracker to prevent double-claim
meter.claimed_erc_generation += energy_amount;
```

**Key Point:** `claimed_erc_generation` tracks what's already been certified.

### 2. Cross-Program Invocation (CPI)

#### Energy Token → Registry
```rust
// In energy_token::mint_grid_tokens()
let cpi_program = ctx.accounts.registry_program.to_account_info();
let cpi_accounts = registry::cpi::accounts::SettleMeterBalance {
    meter_account: ctx.accounts.meter_account.to_account_info(),
    meter_owner: ctx.accounts.meter_owner.to_account_info(),
};
let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

let tokens_to_mint = registry::cpi::settle_meter_balance(cpi_ctx)?;
```

**Key Point:** CPI allows programs to call each other while maintaining security.

### 3. PDA Signing

```rust
// In energy_token::mint_grid_tokens()
let seeds = &[
    b"token_info".as_ref(),
    &[ctx.bumps.token_info],
];
let signer_seeds = &[&seeds[..]];

let cpi_ctx = CpiContext::new_with_signer(
    token_program,
    cpi_accounts,
    signer_seeds
);

token::mint_to(cpi_ctx, amount)?;
```

**Key Point:** Only the program can sign with its PDA, preventing unauthorized operations.

### 4. Authority Validation

```rust
// Pattern 1: Direct check
require!(
    ctx.accounts.authority.key() == config.authority,
    ErrorCode::UnauthorizedAuthority
);

// Pattern 2: Using constraints
#[derive(Accounts)]
pub struct MyInstruction<'info> {
    #[account(
        mut,
        has_one = authority @ ErrorCode::UnauthorizedAuthority
    )]
    pub config: Account<'info, Config>,
    
    pub authority: Signer<'info>,
}
```

### 5. Event Emission

```rust
// Define event
#[event]
pub struct GridTokensMinted {
    pub meter_owner: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

// Emit event
emit!(GridTokensMinted {
    meter_owner: ctx.accounts.meter_owner.key(),
    amount: tokens_minted,
    timestamp: Clock::get()?.unix_timestamp,
});
```

**Key Point:** Events provide off-chain monitoring and audit trails.

### 6. State Validation

```rust
// Check operational status
pub fn is_operational(&self) -> bool {
    !self.emergency_paused && !self.maintenance_mode
}

// Check ERC issuance allowed
pub fn can_issue_erc(&self) -> bool {
    self.is_operational() && self.erc_validation_enabled
}

// Use in instruction
require!(
    poa_config.can_issue_erc(),
    ErrorCode::ErcValidationDisabled
);
```

---

## Testing Commands

### Run Full Test Suite
```bash
# All tests
anchor test

# Specific test file
anchor test tests/registry.ts

# With logs
anchor test -- --features "verbose"

# Skip build
anchor test --skip-build
```

### Test Individual Instructions
```bash
# Registry
anchor test tests/registry.ts -t "should register user"
anchor test tests/registry.ts -t "should register meter"

# Energy Token
anchor test tests/energy-token.ts -t "should mint tokens"

# Trading
anchor test tests/trading.ts -t "should match orders"

# Governance
anchor test tests/governance.ts -t "should issue ERC"
```

---

## Useful Links

- **Anchor Documentation:** https://www.anchor-lang.com/
- **Solana Documentation:** https://docs.solana.com/
- **SPL Token Program:** https://spl.solana.com/token
- **Solana Playground:** https://beta.solpg.io/

---

## Security Checklist

Before deploying to production:

- [ ] All programs audited by security firm
- [ ] Authority keys secured (hardware wallet/MPC)
- [ ] Emergency pause mechanism tested
- [ ] PDA seeds validated for uniqueness
- [ ] All CPI calls validated
- [ ] Account ownership checks in place
- [ ] Amount overflow/underflow checks
- [ ] Event emission complete
- [ ] Integration tests passed
- [ ] Load testing completed

---

**End of Quick Reference**
