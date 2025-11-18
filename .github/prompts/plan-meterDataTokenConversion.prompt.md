# Plan: Configure Meter Data to Token Conversion Ratio

## Current State

The system currently uses a **hardcoded 1:1 ratio** (1 kWh = 1 token) in `src/services/blockchain_service.rs:315`. 

**Current Implementation:**
```rust
// Convert kWh to token amount (with 9 decimals)
let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;
```

**Conversion Formula:**
```
1 kWh = 1 token = 1,000,000,000 lamports
```

## Problem

To make the meter data to token conversion ratio configurable, we need to add a conversion factor that can be adjusted without code changes. This allows the system to support different tokenization models:
- **1:1 ratio** (1.0): 1 kWh = 1 token (current)
- **0.5 ratio**: 2 kWh = 1 token (more conservative)
- **2.0 ratio**: 1 kWh = 2 tokens (incentivize production)

## Implementation Steps

### Step 1: Add Conversion Configuration

**File:** `src/config/mod.rs`

Add a new field to the configuration structure:
- Field name: `token_conversion_rate: f64`
- Environment variable: `TOKEN_CONVERSION_RATE`
- Default value: `1.0` (maintains current 1:1 behavior)
- Description: Number of tokens per kWh of energy

**Example:**
```rust
pub struct Config {
    // ... existing fields ...
    pub token_conversion_rate: f64,
}
```

Load from environment:
```rust
token_conversion_rate: env::var("TOKEN_CONVERSION_RATE")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(1.0),
```

### Step 2: Update BlockchainService

**File:** `src/services/blockchain_service.rs:305`

Modify the `mint_energy_tokens()` function to:
1. Accept a `conversion_rate: f64` parameter
2. Update the calculation on line 315 to apply the conversion rate

**Current signature:**
```rust
pub async fn mint_energy_tokens(
    &self,
    authority: &Keypair,
    user_token_account: &Pubkey,
    mint: &Pubkey,
    amount_kwh: f64,
) -> Result<Signature>
```

**New signature:**
```rust
pub async fn mint_energy_tokens(
    &self,
    authority: &Keypair,
    user_token_account: &Pubkey,
    mint: &Pubkey,
    amount_kwh: f64,
    conversion_rate: f64,
) -> Result<Signature>
```

**Updated calculation:**
```rust
// Convert kWh to token amount (with 9 decimals) using configured conversion rate
let amount_lamports = (amount_kwh * conversion_rate * 1_000_000_000.0) as u64;
```

**Update logging:**
```rust
info!(
    "Minting {} kWh as {} tokens (rate: {}) to {}",
    amount_kwh,
    amount_kwh * conversion_rate,
    conversion_rate,
    user_token_account
);
```

### Step 3: Pass Conversion Rate in Handlers

Update both minting endpoints to pass the conversion rate from configuration.

**File 1:** `src/handlers/token.rs:425` (User self-minting endpoint)

Locate the call to `blockchain_service.mint_energy_tokens()` and add the conversion rate parameter:
```rust
let signature = app_state
    .blockchain_service
    .mint_energy_tokens(
        &authority_keypair,
        &user_token_account,
        &mint_pubkey,
        kwh_amount,
        app_state.config.token_conversion_rate,  // <-- Add this
    )
    .await
    .map_err(|e| AppError::internal(format!("Failed to mint tokens: {}", e)))?;
```

**File 2:** `src/handlers/meters.rs:495` (Admin minting endpoint)

Similar update for the admin endpoint:
```rust
let signature = app_state
    .blockchain_service
    .mint_energy_tokens(
        &authority_keypair,
        &user_token_account,
        &mint_pubkey,
        kwh_amount,
        app_state.config.token_conversion_rate,  // <-- Add this
    )
    .await
    .map_err(|e| AppError::internal(format!("Failed to mint tokens: {}", e)))?;
```

### Step 4: Add Configuration to Environment Files

**File:** `local.env`

Add the new environment variable with default value:
```bash
# Token Conversion Rate (tokens per kWh)
# 1.0 = 1 kWh generates 1 token (default)
# 0.5 = 2 kWh generates 1 token
# 2.0 = 1 kWh generates 2 tokens
TOKEN_CONVERSION_RATE=1.0
```

### Step 5: Update Documentation

**File:** `.github/copilot-instructions.md`

Add to the "Environment Configuration" section:

```markdown
### Token Conversion Configuration
TOKEN_CONVERSION_RATE="1.0"  # Tokens minted per kWh (default: 1.0 = 1:1 ratio)
```

Add to the "Critical Dependencies" or "Token Minting Flow" section:

```markdown
### Conversion Rate Mechanics
- Configurable via `TOKEN_CONVERSION_RATE` environment variable
- Default: 1.0 (1 kWh = 1 token)
- Applied during minting: `tokens = kWh * conversion_rate`
- Examples:
  - Rate 1.0: 25 kWh → 25 tokens
  - Rate 0.5: 25 kWh → 12.5 tokens
  - Rate 2.0: 25 kWh → 50 tokens
```

## Further Considerations

### 1. Conversion Rate Bounds

**Question:** Should the system enforce minimum/maximum conversion rates to prevent misconfigurations?

**Options:**
- **A. No bounds (full flexibility):** Allow administrators to set any positive value, trusting operational controls
- **B. Soft bounds (0.1-10.0):** Warn if outside range but allow override
- **C. Hard bounds (0.1-10.0):** Reject invalid values and fall back to default

**Recommendation:** Start with Option A (no bounds) since this is a trusted internal configuration. Add validation in a future iteration if needed.

**Implementation (if Option C chosen):**
```rust
let rate = env::var("TOKEN_CONVERSION_RATE")
    .ok()
    .and_then(|s| s.parse::<f64>().ok())
    .unwrap_or(1.0);

token_conversion_rate: if rate < 0.1 || rate > 10.0 {
    warn!("TOKEN_CONVERSION_RATE {} out of bounds [0.1-10.0], using default 1.0", rate);
    1.0
} else {
    rate
}
```

### 2. Audit Trail

**Question:** Should the actual conversion rate used be stored in the database for historical tracking and compliance?

**Current State:** The `meter_readings` table stores:
- `kwh_amount` (input)
- `mint_tx_signature` (blockchain proof)
- But NOT the conversion rate applied

**Proposal:** Add `conversion_rate_applied: Option<f64>` column to `meter_readings` table

**Benefits:**
- Full audit trail: "Reading X was minted at rate Y on date Z"
- Historical analysis: Track rate changes over time
- Compliance: Prove exact calculation for any past transaction
- Debugging: Verify correct rate was used

**Migration:**
```sql
-- migrations/YYYYMMDDHHMMSS_add_conversion_rate_to_meter_readings.sql
ALTER TABLE meter_readings 
ADD COLUMN conversion_rate_applied NUMERIC(10,4) DEFAULT NULL;

COMMENT ON COLUMN meter_readings.conversion_rate_applied IS 
'Token conversion rate used during minting (tokens per kWh). NULL if not minted yet.';
```

**Code Update:**
```rust
// After minting, update database
sqlx::query!(
    r#"
    UPDATE meter_readings 
    SET minted = TRUE,
        mint_tx_signature = $1,
        conversion_rate_applied = $2,
        updated_at = NOW()
    WHERE id = $3
    "#,
    signature.to_string(),
    conversion_rate,  // <-- Store the rate used
    reading_id
)
.execute(&app_state.db)
.await?;
```

**Recommendation:** Implement this in the same PR as the conversion rate feature for complete tracking from day one.

### 3. Dynamic Rates (Future Enhancement)

**Vision:** Instead of a single global rate, support time-of-day or market-based conversion rates.

**Use Cases:**
- **Peak hours:** Higher conversion rate (e.g., 1.5x) to incentivize production during high demand
- **Off-peak:** Lower rate (e.g., 0.8x) to discourage excess generation
- **Seasonal adjustments:** Summer vs. winter rates
- **Market-driven:** Integrate with real-time energy spot prices

**Architecture:**
```
Environment Variable (global fallback)
  ↓
Database Table: conversion_rate_schedule
  - time_of_day_start/end
  - day_of_week
  - seasonal_factor
  - base_rate
  ↓
Service: ConversionRateService
  - get_current_rate() → queries schedule
  - get_rate_for_timestamp(DateTime) → historical lookup
  ↓
Minting Flow
```

**Database Schema:**
```sql
CREATE TABLE conversion_rate_schedules (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    base_rate NUMERIC(10,4) NOT NULL,
    time_of_day_start TIME,
    time_of_day_end TIME,
    day_of_week_mask INTEGER,  -- Bitmask: Mon=1, Tue=2, ..., Sun=64
    seasonal_multiplier NUMERIC(5,3) DEFAULT 1.0,
    active BOOLEAN DEFAULT TRUE,
    priority INTEGER DEFAULT 0,  -- Higher priority wins for overlaps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Implementation Priority:** Phase 2 or 3 - after core conversion rate feature is stable and tested.

## Testing Strategy

### Unit Tests

Add tests in `src/services/blockchain_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_rate_calculation() {
        // Test 1:1 ratio (default)
        let kwh = 25.0;
        let rate = 1.0;
        let lamports = (kwh * rate * 1_000_000_000.0) as u64;
        assert_eq!(lamports, 25_000_000_000);

        // Test 0.5 ratio (2 kWh per token)
        let rate = 0.5;
        let lamports = (kwh * rate * 1_000_000_000.0) as u64;
        assert_eq!(lamports, 12_500_000_000);

        // Test 2.0 ratio (1 kWh for 2 tokens)
        let rate = 2.0;
        let lamports = (kwh * rate * 1_000_000_000.0) as u64;
        assert_eq!(lamports, 50_000_000_000);
    }
}
```

### Integration Tests

Add test cases to `scripts/test-complete-flow.sh`:

```bash
# Test different conversion rates
echo "Testing conversion rate scenarios..."

# Scenario 1: Default 1:1 ratio (25 kWh → 25 tokens)
export TOKEN_CONVERSION_RATE=1.0
# ... submit reading, mint, verify on-chain balance

# Scenario 2: Conservative 0.5 ratio (25 kWh → 12.5 tokens)
export TOKEN_CONVERSION_RATE=0.5
# ... submit reading, mint, verify on-chain balance

# Scenario 3: Incentivized 2.0 ratio (25 kWh → 50 tokens)
export TOKEN_CONVERSION_RATE=2.0
# ... submit reading, mint, verify on-chain balance
```

### Manual Testing Checklist

- [ ] Configuration loads correctly from `TOKEN_CONVERSION_RATE` env var
- [ ] Default value (1.0) is applied when env var not set
- [ ] Minting with rate 1.0 produces expected token amount (1:1)
- [ ] Minting with rate 0.5 produces half tokens
- [ ] Minting with rate 2.0 produces double tokens
- [ ] Logs show correct conversion rate and calculated token amount
- [ ] Database stores correct `conversion_rate_applied` value (if implemented)
- [ ] On-chain token balance matches expected amount
- [ ] Multiple readings with different rates work independently

## Rollout Plan

### Phase 1: Core Implementation (This PR)
1. Add `token_conversion_rate` to config
2. Update `mint_energy_tokens()` signature
3. Pass rate in handlers
4. Add to `local.env` with default 1.0
5. Update documentation
6. Add unit tests

### Phase 2: Audit Trail (Next PR)
1. Create migration for `conversion_rate_applied` column
2. Update minting handlers to store rate
3. Add database query tests
4. Update API documentation

### Phase 3: Dynamic Rates (Future)
1. Design `conversion_rate_schedules` table
2. Implement `ConversionRateService`
3. Create admin API for rate management
4. Add scheduling logic
5. Build admin UI for rate configuration

## Success Criteria

- ✅ System supports configurable conversion rate via environment variable
- ✅ Default behavior (1:1 ratio) is preserved
- ✅ All existing tests pass
- ✅ New unit tests validate conversion calculations
- ✅ Documentation clearly explains configuration options
- ✅ Zero breaking changes to existing API contracts
- ✅ Logs provide visibility into conversion rate applied
- ✅ (Optional) Database audit trail stores historical rates

## Questions for Refinement

1. **Precision Concerns:** Should we address the floating-point precision issue mentioned in the research (converting `BigDecimal` → `f64` → `u64`)? Or is current precision acceptable?

2. **Admin API:** Should there be an admin endpoint to view/update the conversion rate at runtime (without restarting the service)?

3. **Per-User Rates:** Future consideration - should different user types (residential vs. commercial) have different conversion rates?

4. **Rate Change Notification:** Should users be notified when the global conversion rate changes? Via email, dashboard banner, etc.?

5. **Backwards Compatibility:** What happens to readings submitted before this feature but minted after? Assumption: They use the current rate at time of minting.
