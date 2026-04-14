# syntax=docker/dockerfile:1.6

# Docker requires a default here even when CI passes the pinned repo toolchain explicitly.
ARG RUST_VERSION=1.93.0

# Build stage
FROM public.ecr.aws/docker/library/rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian13:nonroot

WORKDIR /app

COPY --from=builder --chown=65532:65532 /app/target/release/pki-api /app/pki-api

USER 65532:65532

ENTRYPOINT ["/app/pki-api"]
