# Multi-stage build for PFCP Proxy/Load Balancer
# Using Rust 1.90 to match rs-pfcp dependency requirements
FROM rust:1.90-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files for all binaries to cache dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/test-upf.rs && \
    echo "fn main() {}" > src/bin/test-smf.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/pfcp-proxy /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 pfcp && \
    chown -R pfcp:pfcp /app

USER pfcp

# Expose PFCP port
EXPOSE 8805/udp

# Health check
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD pgrep -x pfcp-proxy || exit 1

ENTRYPOINT ["pfcp-proxy"]
# Users must provide --backends flag
# Example: docker run pfcp-proxy --listen 0.0.0.0:8805 --backends 10.0.1.10:8805,10.0.1.11:8805
# Without backends, the proxy will show an error and exit
