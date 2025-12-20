# GridTokenX API Gateway

High-performance API Gateway for the GridTokenX Energy Trading Platform, built with Rust, Axum, and Solana.

## Prerequisites

- Rust (latest stable)
- Docker & Docker Compose
- Solana CLI (for localnet development)

## Quick Start

1.  **Environment Setup**:
    Copy the example configuration:
    ```bash
    cp .env.example .env
    ```
    Edit `.env` and ensure `JWT_SECRET` is set to a secure random string (not the default "supersecretjwtkey" if in production).

2.  **Infrastructure**:
    Start PostgreSQL and Redis:
    ```bash
    make docker-up
    ```

3.  **Run Development Server**:
    ```bash
    make dev
    ```

## Development Commands

We use `make` to standardize common tasks:

| Command | Description |
|---------|-------------|
| `make dev` | Run the API Gateway locally |
| `make check` | Check for compilation errors |
| `make test` | Run all tests (unit & integration) |
| `make test-integration` | Run specific blockchain integration tests |
| `make format` | Check code formatting |
| `make lint` | Run clippy linter |
| `make localnet` | Start a local Solana validator node |
| `make docker-up` | Start Postgres & Redis dependencies |
| `make docker-down` | Stop Docker services |

## Testing

Integration tests require:
1. A valid keypair file. A mock `dev-wallet.json` is generated for this purpose.
2. A running Solana Localnet (for `token_minting_test`).

To run tests:
```bash
make test
```

## Security Note

- **Secrets**: Do **not** commit `.env` or any keypair JSON files.
- **Config**: The application will panic at startup if a weak `JWT_SECRET` is detected in a non-development environment.
