# Indras Network Multi-Stage Dockerfile
#
# Builds the simulation runner and examples for multi-node testing
# Usage:
#   docker build -t indras-network .
#   docker-compose up
#
# Build stages:
#   1. builder - Compiles Rust binaries
#   2. runtime - Minimal runtime image with binaries

# ============================================================================
# Stage 1: Builder
# ============================================================================
FROM rust:1.86-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY simulation ./simulation
COPY examples ./examples

# Build all binaries in release mode
RUN cargo build --release \
    --bin lua_runner \
    --package sync-demo \
    --package chat-app

# ============================================================================
# Stage 2: Runtime
# ============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash indras

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /build/target/release/lua_runner /app/bin/
COPY --from=builder /build/target/release/sync-demo /app/bin/
COPY --from=builder /build/target/release/chat-app /app/bin/

# Copy simulation scripts
COPY --from=builder /build/simulation/scripts /app/scripts

# Copy docker test scripts
COPY docker/scripts /app/scripts/

# Set ownership
RUN chown -R indras:indras /app

USER indras

# Add binaries to PATH
ENV PATH="/app/bin:${PATH}"

# Data directory for node state
VOLUME /app/data

# Default command
CMD ["lua_runner", "--help"]
