## Plan: Improve Large Files in GridTokenX API Gateway

Based on detailed analysis, here are the specific refactoring strategies for your largest files, prioritized by impact.

---

### 1. `src/services/order_matching_engine.rs` (1525 lines) — **Highest Priority**

| New Module | Content | Target Lines |
|------------|---------|--------------|
| `order_matching/mod.rs` | Re-exports | ~20 |
| `order_matching/order_book.rs` | `OrderBook`, `PriceLevel` structs | ~250 |
| `order_matching/book_order.rs` | `BookOrder` struct, order validation | ~100 |
| `order_matching/clearing_engine.rs` | `MarketClearingEngine` core logic | ~400 |
| `order_matching/match_executor.rs` | Trade execution, settlement integration | ~200 |
| `order_matching/validation.rs` | Input validation | ~100 |
| `order_matching/types.rs` | `OrderSide`, `ClearingPrice`, `TradeMatch` | ~80 |

---

### 2. `src/handlers/registry.rs` (~1200 lines)

| New Module | Content | Target Lines |
|------------|---------|--------------|
| `handlers/registry/mod.rs` | Re-exports | ~20 |
| `handlers/registry/registration.rs` | `register`, `RegisterRequest` | ~180 |
| `handlers/registry/wallet.rs` | Wallet address CRUD | ~150 |
| `handlers/registry/admin.rs` | Admin user operations | ~250 |
| `handlers/registry/activity.rs` | Activity tracking | ~150 |
| `handlers/registry/meters.rs` | Meter registration | ~250 |
| `handlers/registry/types.rs` | Shared DTOs | ~100 |

---

### 3. `src/main.rs` (794 lines)

| New Module | Content | Target Lines |
|------------|---------|--------------|
| `src/app_state.rs` | `AppState` struct, `FromRef` impls, builder | ~80 |
| `src/router/mod.rs` | Route composition | ~40 |
| `src/router/public.rs` | Public routes (health, auth) | ~60 |
| `src/router/protected.rs` | Protected routes (trading, meters) | ~120 |
| `src/router/admin.rs` | Admin routes | ~70 |
| `src/startup.rs` | Service init, DB/Redis setup, background tasks | ~200 |

**Target `main.rs`: ~100 lines** (entry point only)

---

### 4. `src/error.rs` (646 lines)

| New Module | Content | Target Lines |
|------------|---------|--------------|
| `src/error/mod.rs` | Re-exports | ~30 |
| `src/error/codes.rs` | `ErrorCode` enum (42 variants), `code()`/`message()` | ~200 |
| `src/error/api_error.rs` | `ApiError` enum, `IntoResponse` impl | ~150 |
| `src/error/response.rs` | `ErrorResponse`, `ErrorDetail` structs | ~50 |
| `src/error/helpers.rs` | Constructor helpers, rejection handling | ~80 |

---

### 5. `src/services/settlement_service.rs` (698 lines)

| New Module | Content | Target Lines |
|------------|---------|--------------|
| `settlement/mod.rs` | Re-exports | ~20 |
| `settlement/types.rs` | `Settlement`, `SettlementStatus`, `SettlementConfig` | ~100 |
| `settlement/service.rs` | `SettlementService` core logic | ~300 |
| `settlement/blockchain.rs` | On-chain settlement execution | ~150 |
| `settlement/retry.rs` | Retry logic, failed settlement handling | ~100 |

---

### Steps

1. **Extract `AppState` to `src/app_state.rs`** — Move `AppState` struct and `FromRef` implementations from `main.rs` to dedicated module. Add builder pattern for cleaner initialization.

2. **Create `src/router/` directory structure** — Split route definitions from `main.rs` into `public.rs`, `protected.rs`, and `admin.rs`. Use `mod.rs` for composition.

3. **Split `src/error.rs` into domain modules** — Separate `ErrorCode` enum, `ApiError` enum, response types, and helper functions into logical submodules.

4. **Refactor `order_matching_engine.rs`** — Extract `OrderBook`, `BookOrder`, `MarketClearingEngine`, and match execution into separate files under `order_matching/` directory.

5. **Split `handlers/registry.rs`** — Separate registration, wallet, admin, activity, and meter handlers into dedicated submodules.

6. **Refactor `settlement_service.rs`** — Extract types, core service logic, blockchain integration, and retry handling into `settlement/` directory.

---

### Further Considerations

1. **Use `mod.rs` vs file-based modules?** — Recommend directory modules (`mod.rs`) for multi-file splits / Use `mod.rs` pattern consistently
2. **Breaking changes for imports?** — Use re-exports in `mod.rs` to maintain API compatibility / Accept some churn
3. **Test file organization?** — Keep tests alongside modules in `#[cfg(test)]` / Move to `tests/unit/` structure
