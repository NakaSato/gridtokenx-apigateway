# Build Stage
FROM rust:1.81-slim-bookworm as builder

WORKDIR /usr/src/app
COPY . .

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev libpq-dev

# Build the application
# We use --bin api-gateway to specifically build the gateway binary
RUN cargo build --release --bin api-gateway

# Runtime Stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y libssl-dev libpq-dev ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/app/target/release/api-gateway /app/api-gateway

# Expose port (adjust if needed)
EXPOSE 4000

# Run the binary
CMD ["./api-gateway"]
