# Multi-stage Dockerfile for music_evo
# Builds for the current architecture (supports both x86_64 and arm64)

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM rust:1.75-bookworm AS builder

# Install build dependencies
# - libasound2-dev: Required by rodio for ALSA backend
# - pkg-config: Required for finding system libraries
RUN apt-get update && apt-get install -y \
    libasound2-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir -p src/bin && \
    echo 'fn main() { println!("dummy"); }' > src/bin/main_web.rs && \
    echo 'fn main() { println!("dummy"); }' > src/bin/init_experiment.rs && \
    echo 'fn main() { println!("dummy"); }' > src/bin/scrub_db.rs && \
    echo 'fn main() { println!("dummy"); }' > src/bin/reproduce.rs && \
    echo 'pub fn dummy() {}' > src/lib.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release --bin web_server && \
    rm -rf src && \
    rm -rf target/release/deps/music_evo* && \
    rm -rf target/release/.fingerprint/music_evo* && \
    rm -rf target/release/web_server* target/release/init_experiment* target/release/scrub_db* target/release/reproduce*

# Copy actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release --bin web_server --bin init_experiment --bin scrub_db --bin reproduce

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
# - libasound2: ALSA runtime library (required by rodio)
# - ca-certificates: For HTTPS connections
RUN apt-get update && apt-get install -y \
    libasound2 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd --create-home --shell /bin/bash appuser

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/web_server /app/
COPY --from=builder /app/target/release/init_experiment /app/
COPY --from=builder /app/target/release/scrub_db /app/
COPY --from=builder /app/target/release/reproduce /app/

# Copy static assets and configuration
COPY habitat_config.json /app/
COPY static/ /app/static/

# Create directories for WAV output
RUN mkdir -p /app/current_generation && chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose Rocket default port
EXPOSE 8000

# Default command
CMD ["./web_server"]
