# GridTokenX API Gateway - Copilot Instructions

This document guides AI agents in developing on the GridTokenX P2P energy trading platform.

## System Architecture Overview

**Core Stack**: Rust/Axum web framework + PostgreSQL + Redis + Solana blockchain (Anchor)

**Major Components**:
1. **API Gateway** (`src/main.rs`, `src/lib.rs`) - REST/WebSocket server on port 8080
2. **Market Clearing Engine** (`src/services/market_clearing.rs`) - Continuous order matching with price-time priority
3. **Settlement Service** (`src/services/settlement_blockchain_service.rs`) - Submits trades to Solana blockchain
4. **Order Matching Engine** (`src/services/order_matching_engine.rs`) - In-memory order book (BTreeMap)
5. **Blockchain Service** (`src/services/blockchain_service.rs`) - Solana RPC interactions
6. **Meter Service** (`src/services/meter_service.rs`) - Energy reading verification and management

**Data Flow**: User creates order → Order stored in PostgreSQL → Market clearing loop matches orders every 1000ms → Settlement service submits to blockchain → Trade confirmed

## Key Developer Workflows

### Building & Running
```bash
make install          # Install deps + build
make dev             # Start dev server (with hot reload via `cargo watch`)
make build           # Release build
cargo run            # Direct run
```

### Testing Strategy
Run tests in this order:
```bash
make test-unit                 # Fast: ~5s
make test-quick                # Smoke: blockchain + flow (01-02)
make test-integration          # Full: 01-03 scripts
make test-e2e                  # Market clearing + settlement (04-07)
make test-full                 # Complete suite (unit + all scripts)
```

**Test Scripts** (in `/scripts/`):
- `01-test-complete-flow.sh` - User registration, login, order creation
- `05-test-market-clearing.sh` - Order matching and execution
- `06-test-settlement-flow.sh` - Blockchain settlement

### Database
```bash
make migrate          # Run migrations from `/migrations/`
make db-reset        # Drop and recreate
```

Migrations use SQLx with compile-time checked queries. Always run `cargo sqlx prepare` after changing queries for offline mode.

### Local Infrastructure
PostgreSQL runs on Docker (port 5432, user: `gridtokenx_user`, db: `gridtokenx`). Check `.env.example` for defaults. Set `TEST_MODE=true` in config to bypass email verification.

## Critical Patterns & Conventions

### Error Handling
Use the custom `ApiError` enum with structured error codes (see `src/error.rs`):
- Auth errors: `AUTH_1xxx` (e.g., `EmailNotVerified → 401`)
- Validation errors: `VAL_3xxx` (e.g., `InvalidWalletAddress`)
- Blockchain errors: `BLOCKCHAIN_9xxx`

**Always return appropriate HTTP status** via `IntoResponse`. Example: email verification failures return 401 (not 500) after fix at line ~100 of error.rs.

### Order Matching Algorithm
Located in `src/services/order_matching_engine.rs`:
- **Price-Time Priority**: Highest buy price matched with lowest sell price; ties broken by order creation time
- **Partial Fills**: Orders can be partially filled; status tracked as `Pending → PartiallyFilled → Filled`
- **Execution**: Loop runs every 1000ms; matches all possible pairs per iteration

### Blockchain Settlement
Settlement transactions are idempotent (no duplicates even if retried):
1. Market clearing creates `Settlement` records in PostgreSQL
2. `SettlementBlockchainService.process_pending_settlements()` queries pending, submits to Solana
3. Monitors confirmation (30 attempts × 2s = 60s timeout); auto-retries up to 3 times
4. Status: `pending → submitted → confirmed` (see migration `20241110000001_add_settlement_transactions`)

**Key concern**: BigDecimal to lamports conversion; use `settlement.amount_energy * LAMPORTS_PER_SOL`

### Authentication & Authorization
- **JWT tokens**: 24-hour expiry (configurable). Middleware extracts from `Authorization: Bearer <token>`
- **Email verification**: Required unless `TEST_MODE=true`. Bypassing returns `AUTH_1005: EmailNotVerified`
- **Role-based**: Admin vs. user permissions enforced in handlers

### AppState Initialization
`AppState` (in `main.rs`) is cloned across handlers. Initialize all services there:
```rust
pub struct AppState {
    pub db: sqlx::PgPool,
    pub redis: redis::Client,
    pub market_clearing_engine: services::OrderMatchingEngine,
    pub blockchain_service: services::BlockchainService,
    // ... add services here before passing to router
}
```

## Common Development Tasks

### Adding a New Endpoint
1. Create handler in `src/handlers/` module
2. Define request/response models in `src/models/`
3. Register route in `main.rs` router: `Router::new().route("/api/path", post(handler))`
4. Add to OpenAPI spec via `#[utoipa::path(...)]` decorator
5. Test with provided test script

### Modifying Order Matching Logic
Edit `src/services/order_matching_engine.rs`:
- `match_orders()` implements the core matching algorithm
- Call from market clearing service's continuous loop (1000ms interval)
- Results create `Trade` records in PostgreSQL + send WebSocket updates

### Adding Database Fields
1. Create migration in `migrations/TIMESTAMP_description.sql` (use SQLx naming)
2. Update corresponding model in `src/models/`
3. Update schema in `src/database/schema.rs` if needed
4. Run `make migrate` to apply

### Integrating Blockchain Changes
Changes to Anchor programs require:
1. Rebuild Anchor programs (separate repo)
2. Update `solana_rpc_url` and program IDs in config
3. Rebuild IDL, update service if CPI signatures change
4. Test with `make test-01-blockchain` first

## File Structure Reference

| Path | Purpose |
|------|---------|
| `src/main.rs` | App initialization, router setup, AppState creation |
| `src/services/` | Business logic (market clearing, settlement, auth) |
| `src/handlers/` | HTTP request/response handlers organized by domain |
| `src/models/` | Request/response DTOs and database models |
| `src/middleware/` | Authentication, logging, CORS middleware |
| `src/auth/jwt.rs` | JWT token generation and verification |
| `src/error.rs` | Error types and HTTP response mapping |
| `migrations/` | SQLx database migrations (ordered by timestamp) |
| `docs/technical/` | Architecture diagrams, guides, and design docs |
| `scripts/` | Integration test scripts (01-10 suite) |

## Integration Points & External Dependencies

- **Solana RPC**: `SOLANA_RPC_URL` (devnet/mainnet-beta configurable)
- **Energy Token Program**: Program ID in config; must exist before settlement
- **PostgreSQL**: SQLx with compile-time query verification; TimescaleDB optional for metrics
- **Redis**: Used for order book snapshots and session caching
- **Email Service** (optional): Lettre for verification emails; disabled if `SMTP_HOST` not set

## Performance Considerations

- **Order Book**: In-memory BTreeMap (not persisted between restarts; recovered from Redis snapshots)
- **Market Clearing**: Runs every 1000ms; batch processes all matches per cycle
- **Blockchain**: Solana transactions batched; monitor failed submissions via retry logic
- **Caching**: Trade history, user balances cached in Redis with TTL

## Testing Guidelines

- Unit tests in `tests/unit/`; run with `cargo test --lib`
- Integration tests are shell scripts in `scripts/`; each tests one component
- E2E tests (04-07) require running server + full blockchain integration
- Logs: Set `RUST_LOG=debug` for detailed traces; `RUST_LOG=info` for production

## Common Gotchas

1. **Email verification blocking login**: Must explicitly set `TEST_MODE=true` or verify via database
2. **Settlement idempotency**: Always check for existing `settlement_transaction` before submitting new one
3. **Market clearing not triggering**: Ensure epoch exists and is in correct status (see `DOC_EPOCH_MANAGEMENT.md`)
4. **Order book inconsistency**: In-memory state not synced after Redis restart; rebuild from database required
5. **Blockchain timeout**: 60s default; increase for congested networks via `confirmation_timeout` config

## References

- **Architecture**: `docs/technical/architecture/system/SYSTEM_ARCHITECTURE.md`
- **Market Clearing**: `docs/technical/MARKET_CLEARING_ENGINE.md`
- **Settlement**: `docs/blockchain/SETTLEMENT_BLOCKCHAIN_IMPLEMENTATION_COMPLETE.md`
- **Anchor Programs**: `docs/technical/architecture/blockchain/anchor-programs/`
- **API Spec**: `openapi-spec.json` (generated from code)

---

*Last updated: November 2025. For implementation examples, reference test scripts and handler implementations in the codebase.*
