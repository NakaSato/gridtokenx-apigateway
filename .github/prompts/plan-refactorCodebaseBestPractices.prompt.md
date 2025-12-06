## Plan: Refactor GridTokenX API Gateway to Best Practices

Comprehensive refactoring of the Rust API gateway to improve code quality, eliminate technical debt, and align with Rust best practices. The codebase has a solid foundation (Axum framework, layered architecture) but needs cleanup of duplicate definitions, unsafe patterns, debug code, and improved test infrastructure.

### Steps

1. **Fix critical structural issues** — Consolidate duplicate `AppState` definitions in [`main.rs`](src/main.rs) and [`lib.rs`](src/lib.rs) into a single source, fix `edition = "2024"` to `"2021"` in [`Cargo.toml`](Cargo.toml), and align Solana crate versions.

2. **Eliminate unsafe code and `unwrap()` patterns** — Replace 50+ `unwrap()`/`expect()` calls in production code with proper `?` operator or error handling, and refactor 5 `unsafe` blocks in [`auth/jwt.rs`](src/auth/jwt.rs) and [`config/tokenization.rs`](src/config/tokenization.rs) to use test isolation crates.

3. **Remove debug code and dead code** — Strip 30+ `println!`/`dbg!` statements (especially in [`main.rs`](src/main.rs), [`handlers/auth.rs`](src/handlers/auth.rs)), audit 30+ `#[allow(dead_code)]` suppressions, and remove or integrate unused code.

4. **Refactor large modules** — Extract router setup from [`main.rs`](src/main.rs) (794 lines) into `router.rs`, break down complex functions like `build_openapi_spec`, and address 14+ TODO/FIXME items.

5. **Improve error handling consistency** — Standardize usage of `thiserror` across services, ensure error context is preserved in conversions, and reduce boilerplate in [`error.rs`](src/error.rs) (646 lines).

6. **Enhance test infrastructure** — Leverage unused `testcontainers` dependency, extract inline `#[cfg(test)]` modules to `tests/unit/`, add handler tests beyond [`auth_tests.rs`](tests/unit/handlers/auth_tests.rs), and create mock implementations using traits.

### Further Considerations

1. **Restructure to Domain-Driven Design?** — Keep current layered structure (simpler) / Migrate to `domain/`, `infrastructure/`, `app/` folders (cleaner boundaries) / Hybrid approach with gradual migration
2. **How to handle 14+ incomplete TODOs?** — Create GitHub issues for tracking / Complete critical ones (metrics, transaction_coordinator) first / Document as known limitations
3. **Clone-heavy middleware patterns** — Profile performance impact first / Refactor to use references / Accept current state if negligible impact
