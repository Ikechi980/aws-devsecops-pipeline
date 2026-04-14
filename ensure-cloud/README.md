# Ensure Cloud Services

This repository contains Ensure Cloud runtime services and shared local infrastructure used for development and integration testing.

## Services

- **[pki](./pki/)** - Certificate authority API for issuing device certificates
- **[headend-gateway](./headend-gateway/)** - WebSocket gateway for headend devices
- **[core-change-publisher](./core-change-publisher/)** - Lambda for forwarding core change events to Ensure communities
- **[headend-api](./headend-api/)** - Lambda API for headends to access core community data

## Repository Layout

- `pki/` - PKI API service code, Docker assets, scripts, and tests
- `headend-gateway/` - Gateway service code, Docker assets, scripts, and tests
- `headend-api/` - Lambda API code, scripts, and tests
- `core-change-publisher/` - Lambda event publisher code, scripts, and tests
- `infra/` - Shared local infrastructure (docker-compose, localstack init, nginx, step-ca data)
- `scripts/` - Root-level helper scripts for local environment lifecycle

## Runtime Architecture (High Level)

- Headend devices connect through mTLS-enabled entry points.
- `pki` issues certificates using `step-ca`.
- `headend-gateway` receives real-time messages and forwards them to connected headends.
- `core-change-publisher` consumes core change events and publishes headend-targeted messages.
- `headend-api` serves headend-facing API requests for core data and events.

## Prerequisites

- Docker + Docker Compose
- Bash-compatible shell
- Rust toolchain pinned by `rust-toolchain.toml` (for running services/tests directly with `cargo`)
- `cargo-lambda 1.9.1` for local Lambda development and tests

Use the pinned local toolchain before running the Lambda service scripts:

```bash
rustup toolchain install 1.93.0 --profile minimal --component rustfmt --component clippy
cargo install cargo-lambda --locked --version 1.9.1
```

## Local Development

Shared infrastructure (Step CA, mock-systems-api, mock-core-resources-api, MongoDB, LocalStack, nginx) lives under `infra/` and is orchestrated from root scripts.

Run the pre-merge gates locally:
```bash
./scripts/check.sh all
```

Run an individual gate:
```bash
./scripts/check.sh security
./scripts/check.sh lint
./scripts/check.sh test
```

Start shared infrastructure only:
```bash
./scripts/dev.sh run
```

Run a service with its dependencies:
```bash
cd pki
./scripts/dev.sh run

cd headend-gateway
./scripts/dev.sh run

cd core-change-publisher
./scripts/dev.sh run

cd headend-api
./scripts/dev.sh run
```

Reset all shared dev data:
```bash
./scripts/dev.sh reset
```

Run the full ensure stack for cross-repo integration testing:
```bash
./scripts/integrated-dev.sh run
```

## Service-Specific Docs

Each service has its own README with environment variables, endpoint details, and test commands:

- `pki/README.md`
- `headend-gateway/README.md`
- `headend-api/README.md`
- `core-change-publisher/README.md`
