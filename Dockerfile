# Multi-stage build for teleport (Rust)

# --- Build stage ---
FROM rust:1.95-slim-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler pkg-config && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt

WORKDIR /app
COPY . .

WORKDIR /app/teleport
RUN cargo build --release

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
