# Plan: Add Meter Registration Flow

Create a secure meter registration system that links physical smart meters to user accounts, validates meter identity, and stores meter metadata for signature verification in future data submissions.

## Steps

1. **Create database migration** for `meters` table in `migrations/` with columns: `id` (UUID), `user_id` (FK to users), `meter_id` (VARCHAR(50) UNIQUE), `meter_pubkey` (VARCHAR(44)), `meter_type` (ENUM), `status` (ENUM), timestamps; add `meter_registration_id` FK column to `meter_readings` table

2. **Define meter models** in new `src/models/meter.rs` with `Meter`, `MeterType` enum (Solar, Wind, Battery, Grid), `MeterStatus` enum (Active, Inactive, Maintenance), `RegisterMeterRequest` with validation (meter_id, meter_pubkey via Solana pubkey format), and `MeterResponse` structs following the pattern in `src/models/energy.rs`

3. **Extend `MeterService`** with `register_meter()` method that validates meter_pubkey using `BlockchainService::parse_pubkey()`, checks meter_id uniqueness, inserts into database with SQLx compile-time checked query, and returns registered meter details

4. **Add `register_meter` handler** in `src/handlers/meters.rs` following the `register` pattern: extract `user_id` from `AuthenticatedUser`, validate request with `validator` crate, call `state.meter_service.register_meter()`, return 201 with `MeterResponse`

5. **Register route and OpenAPI** by adding `POST /api/meters/register` under `.nest("/api/meters", ...)` in `src/main.rs`, add utoipa annotation to handler following `submit_reading` pattern (tag: "meters", security: bearer_auth), register schemas in `openapi.rs`

6. **Update reading submission** to validate meter ownership: modify `submit_reading` to query `meters` table, verify `user_id` matches authenticated user, and link `meter_registration_id` in the inserted `meter_readings` record

## Further Considerations

1. **Meter Type Selection** - Should meter type be user-selected during registration, or auto-detected from first reading? Default to `Solar` for MVP or require explicit selection?

2. **Meter Pubkey Validation** - Store raw pubkey string only, or also validate it matches a deployed MeterAccount PDA on-chain during registration? Defer blockchain verification to future signature validation phase?

3. **Multi-Meter Support** - Allow users to register multiple meters (e.g., solar + battery)? Add `GET /api/meters/my-meters` endpoint, or defer until needed?

## Research Context

### Current System State
- **No `meters` table exists** - meters are currently identified only by `meter_id` string in `meter_readings`
- **No meter public key storage** - cannot verify meter signatures
- **No meter ownership tracking** at database level
- Meter readings link to users via `user_id` column but no meter registration validation

### Database Schema (meter_readings)
```sql
CREATE TABLE meter_readings (
    id UUID PRIMARY KEY,
    meter_id VARCHAR(50) NOT NULL,
    user_id UUID,  -- FK to users
    wallet_address VARCHAR(88) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    kwh_amount NUMERIC(12, 4),
    reading_timestamp TIMESTAMPTZ,
    minted BOOLEAN DEFAULT FALSE,
    mint_tx_signature VARCHAR(88),
    -- ... additional columns
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Current Authentication Pattern
```rust
pub async fn handler(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,  // user.sub = user_id
    Json(request): Json<Request>,
) -> Result<Json<Response>, ApiError> {
    // user.sub contains UUID
    // user.role contains role string
}
```

### Solana Pubkey Validation
```rust
use solana_sdk::pubkey::Pubkey;
let pubkey = BlockchainService::parse_pubkey(&meter_pubkey)
    .map_err(|e| ApiError::BadRequest(format!("Invalid meter pubkey: {}", e)))?;
```

### SQLx Pattern
```rust
let meter = sqlx::query_as!(
    Meter,
    r#"SELECT id, user_id, meter_id, meter_pubkey, 
       meter_type as "meter_type: MeterType",
       status as "status: MeterStatus",
       created_at, updated_at
       FROM meters WHERE meter_id = $1"#,
    meter_id
).fetch_one(&self.db_pool).await?;
```

### OpenAPI Annotation Pattern
```rust
#[utoipa::path(
    post,
    path = "/api/meters/register",
    tag = "meters",
    request_body = RegisterMeterRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 201, description = "Meter registered successfully", body = MeterResponse),
        (status = 400, description = "Invalid meter data or duplicate meter_id"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
```

### Validation Pattern
```rust
use validator::Validate;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterMeterRequest {
    #[validate(length(min = 1, max = 50))]
    pub meter_id: String,
    
    #[validate(length(min = 32, max = 44))]
    pub meter_pubkey: String,  // Solana public key
    
    pub meter_type: MeterType,
}
```

### Service Pattern
```rust
impl MeterService {
    pub async fn register_meter(
        &self,
        user_id: Uuid,
        request: RegisterMeterRequest,
    ) -> Result<Meter> {
        // Validate meter_pubkey format
        BlockchainService::parse_pubkey(&request.meter_pubkey)?;
        
        // Check uniqueness
        let exists = sqlx::query!(...)
            .fetch_optional(&self.db_pool)
            .await?;
            
        if exists.is_some() {
            return Err(anyhow!("Meter ID already registered"));
        }
        
        // Insert
        let meter = sqlx::query_as!(...)
            .fetch_one(&self.db_pool)
            .await?;
            
        Ok(meter)
    }
}
```

### Blockchain Context (Registry Program)
From documentation, the Anchor program has a `register_meter` instruction that creates MeterAccount PDA with:
- Seeds: `[b"meter", meter_id.as_bytes()]`
- MeterAccount fields: meter_id, owner (Pubkey), meter_type, status, energy stats, timestamps
- Meter types: Solar | Wind | Battery | Grid
- Meter status: Active | Inactive | Maintenance

### Files to Modify/Create
1. **New**: `migrations/20241118XXXXXX_create_meters_table.sql`
2. **New**: `src/models/meter.rs`
3. **Modify**: `src/models/mod.rs` - add `pub mod meter;`
4. **Modify**: `src/services/meter_service.rs` - add `register_meter()` method
5. **Modify**: `src/handlers/meters.rs` - add `register_meter` handler
6. **Modify**: `src/main.rs` - add route
7. **Modify**: `src/openapi/mod.rs` - register schemas

### Key Dependencies
- `validator = "0.16"` - request validation
- `sqlx` - compile-time checked queries
- `solana-sdk` - pubkey validation
- `utoipa` - OpenAPI schemas
- `uuid` - meter IDs
- `chrono` - timestamps

### Testing Approach
```bash
# Register meter
curl -X POST http://localhost:8080/api/meters/register \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "meter_id": "METER001",
    "meter_pubkey": "5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP8",
    "meter_type": "Solar"
  }'

# Submit reading (should validate meter ownership)
curl -X POST http://localhost:8080/api/meters/submit-reading \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "meter_id": "METER001",
    "wallet_address": "...",
    "kwh_amount": "50.0",
    "reading_timestamp": "2025-11-18T10:00:00Z"
  }'
```
