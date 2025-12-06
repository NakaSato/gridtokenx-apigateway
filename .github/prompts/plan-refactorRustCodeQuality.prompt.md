## Plan: Refactor Rust Codebase for Quality Improvement

This plan addresses 18 identified issues across the entire codebase, prioritizing security, correctness, and maintainability. The refactoring is organized into phases that can be executed incrementally.

### Steps

1. **Fix Critical Security Issues** - Remove hardcoded UUID in [`src/auth/middleware.rs`](src/auth/middleware.rs):53-56 and move Solana program IDs in [`src/services/blockchain_service.rs`](src/services/blockchain_service.rs):191-219 to configuration/environment variables.

2. **Replace `.expect()`/`.unwrap()` in Production Code** - Convert to proper `?` operator error handling in [`src/services/audit_logger.rs`](src/services/audit_logger.rs):169, [`src/startup.rs`](src/startup.rs):934-972, [`src/services/event_processor_service.rs`](src/services/event_processor_service.rs):455-550, and [`src/handlers/meters.rs`](src/handlers/meters.rs):457-495.

3. **Remove Unused Code** - Delete dead function `verify_api_key` in [`src/auth/middleware.rs`](src/auth/middleware.rs):200, unused constant `TOKEN_PROGRAM_ID` in [`src/services/blockchain_utils.rs`](src/services/blockchain_utils.rs):11, and 20+ unused imports flagged by compiler warnings.

4. **Split Large Service Files** - Extract [`src/services/meter_service.rs`](src/services/meter_service.rs) (1525 lines) into `meter_crud.rs`, `clearing_engine.rs`, `redis_cache.rs`; split [`src/handlers/user_management.rs`](src/handlers/user_management.rs) (1201 lines) into `registration.rs`, `profile.rs`, `wallet.rs`, `role.rs`.

5. **Consolidate Duplicated Patterns** - Extract pagination logic from handlers into [`src/utils/pagination.rs`](src/utils/pagination.rs) generic helpers; centralize activity logging in [`src/services/audit_logger.rs`](src/services/audit_logger.rs).

6. **Add Missing Async Safeguards** - Wrap background tasks in [`src/startup.rs`](src/startup.rs) with `tokio::time::timeout`; add cancellation tokens to polling loops in [`src/services/meter_polling_service.rs`](src/services/meter_polling_service.rs).

### Further Considerations

1. **TODO Items**: 7 incomplete features marked with TODO/FIXME - should these be addressed now or tracked as separate issues?

2. **Test Organization**: Tests currently inline in source files - move to `tests/` directory for cleaner separation, or keep colocated? (Colocated is Rust convention but adds file size)

3. **Documentation Level**: Should we add `///` doc comments to all public APIs now, or defer to a separate documentation pass?
