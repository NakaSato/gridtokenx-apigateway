.PHONY: all dev check test test-integration build clean localnet format lint docker-up docker-down

# Environment defaults
# Ensures integration tests can find the mock wallet
export AUTHORITY_WALLET_PATH ?= dev-wallet.json

all: check test

# Run the application in development mode
dev:
	cargo run

# Check if the code compiles
check:
	cargo check

# Run all tests (unit and integration)
test:
	cargo test -- --nocapture

# Run specific integration tests (requires Solana localnet for full success)
test-integration:
	cargo test --test erc_lifecycle_test -- --nocapture
	cargo test --test token_minting_test -- --nocapture

# Build release binary
build:
	cargo build --release

# Clean build artifacts
clean:
	cargo clean

# Check code formatting
format:
	cargo fmt -- --check

# Run linter
lint:
	cargo clippy -- -D warnings

# Start Solana Test Validator (Localnet)
localnet:
	solana-test-validator -r

# Start database and redis via Docker
docker-up:
	docker-compose up -d postgres redis

# Stop all docker services
docker-down:
	docker-compose down
