# Build stage
FROM rust:1.92 AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock* ./

# Create a dummy main.rs to build dependencies first (for better caching)
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src

# Build the application
RUN touch src/main.rs && cargo build --release

# Runtime stage - using trixie for newer glibc
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/scorekeeper /app/scorekeeper

# Expose the port the app runs on
EXPOSE 8080

# Set the binary as the entrypoint
ENTRYPOINT ["/app/scorekeeper"]
