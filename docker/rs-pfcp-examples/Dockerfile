# Multi-stage build for rs-pfcp example applications
# Builds session-server, session-client, heartbeat-server, heartbeat-client
# Using Rust 1.90 to match rs-pfcp requirements
FROM rust:1.90-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Clone rs-pfcp repository
RUN git clone https://github.com/xandlom/rs-pfcp.git .

# Build all example binaries
RUN cargo build --release --examples

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Copy example binaries from builder
COPY --from=builder /build/target/release/examples/session-server /usr/local/bin/
COPY --from=builder /build/target/release/examples/session-client /usr/local/bin/
COPY --from=builder /build/target/release/examples/heartbeat-server /usr/local/bin/
COPY --from=builder /build/target/release/examples/heartbeat-client /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 pfcp && \
    chown -R pfcp:pfcp /app

USER pfcp

# Default command shows available examples
CMD ["sh", "-c", "echo 'Available examples: session-server, session-client, heartbeat-server, heartbeat-client' && echo 'Usage: docker run <image> <example> [args]'"]
