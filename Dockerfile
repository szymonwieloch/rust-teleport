# Multi-stage build for teleport (Rust)

# --- Build stage ---
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler pkg-config && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy

WORKDIR /app
COPY . .

WORKDIR /app/teleport

# Run checks and tests before building release
RUN cargo fmt --check
RUN cargo clippy --all-targets -- -D warnings
RUN cargo test --release --locked
RUN cargo build --release --locked

# --- Runtime stage ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/teleport/target/release/teleport /usr/local/bin/
COPY --from=builder /app/teleport/target/release/telecli /usr/local/bin/

# Copy default config
COPY cfg/teleport.yaml /etc/teleport/teleport.yaml

EXPOSE 50051

# Run as non-root
RUN useradd --create-home appuser
USER appuser
WORKDIR /home/appuser

ENTRYPOINT ["teleport"]
