# Multi-stage build for Rust API Gateway
FROM rust:1-slim-bookworm AS chef

# Install cargo-chef for dependency caching
RUN cargo install cargo-chef
WORKDIR /app

# Planner stage
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage
FROM chef AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    curl \
    bzip2 \
    ca-certificates \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*



# Install spl-token CLI via Cargo (network safe fallback)
RUN cargo install spl-token-cli


# Enable offline mode for sqlx
ENV SQLX_OFFLINE=true

COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer
ENV CARGO_BUILD_JOBS=2
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code and build application
COPY . .
ENV CARGO_BUILD_JOBS=2
RUN cargo build --release --bin api-gateway

# Runtime stage
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/api-gateway /usr/local/bin/api-gateway
# Copy spl-token binary
COPY --from=builder /usr/local/cargo/bin/spl-token /usr/local/bin/


# Create non-root user
RUN useradd -m -u 1000 apigateway && \
    chown -R apigateway:apigateway /app

USER apigateway

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the application
CMD ["api-gateway"]
