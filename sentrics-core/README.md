# Sentrics Core Services

This repository contains the core backend services for the Sentrics platform. Services are built as independent components that share common infrastructure for local development.

## Services

- **[resources-api](./resources-api/)** - API for managing communities, locations, and residents
- **[yardi-sync](./yardi-sync/)** - Background service that synchronizes Yardi EHR data with resources-api
- **[resources-change-logger](./resources-change-logger/)** - Lambda that stores resources-api change events in DynamoDB

## Local Development

All services share common infrastructure components (PostgreSQL and LocalStack) to simplify development and match the production topology where services run in the same AWS environment.

Use the pinned local toolchain before running the service scripts:

```bash
rustup toolchain install 1.93.0 --profile minimal --component rustfmt --component clippy
cargo install cargo-lambda --locked --version 1.9.1
```

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

### Getting Started

For day-to-day work, use the service scripts. They bring up infrastructure, start dependencies, and shut everything down on Ctrl-C.

For resources-api:
```bash
cd resources-api
./scripts/dev.sh run
```

For yardi-sync:
```bash
cd yardi-sync
./scripts/dev.sh run
```

For resources-change-logger:
```bash
cd resources-change-logger
./scripts/dev.sh run
```

### Shared Infrastructure

The shared infrastructure includes:

- **PostgreSQL** (port 5432) - Used by resources-api for data persistence
- **LocalStack** (port 4566) - Provides local AWS services (SNS, SQS)
  - SNS topic `resources-events` - Published by resources-api when resources change
  - SQS queue `yardi-sync-events` - Subscribed to resources-events, consumed by yardi-sync
  - SQS queue `resources-change-logger-events` - Subscribed to resources-events, consumed by resources-change-logger
  - SNS topic `yardi-sync-failures` - Published by yardi-sync when integration issues occur
  - DynamoDB table `resources-change-log` - Stores change event audit entries

All shared configuration values are defined in `infra/dev.env` and automatically loaded by service scripts.

### Working with Remote Services

You can point individual services at staging or production environments by overriding environment variables in your local `.env` file:

```bash
# In yardi-sync/.env
RESOURCES_API_BASE_URL=https://staging-api.example.com/v1
YARDI_API_BASE_URL=https://sandbox.yardipca.com/your-instance/...
```

This allows you to test integration with real external services while keeping other dependencies local.

### Infrastructure Management

```bash
# Start shared infrastructure and tail logs
./scripts/dev.sh run

# Reset all data (PostgreSQL + LocalStack)
./scripts/dev.sh reset

# Show lock and running status
./scripts/dev.sh status

# Stop infrastructure (use --force if a stale lock exists)
./scripts/dev.sh stop
```

### Integrated Stack Command

To run all core services together for cross-repo integration testing:

```bash
./scripts/integrated-dev.sh run
```

### Running Tests

Tests start the full local environment, including the service itself:

```bash
cd resources-api && ./scripts/dev.sh test
cd yardi-sync && ./scripts/dev.sh test
cd resources-change-logger && ./scripts/dev.sh test
```

## Architecture Notes

Services in this repository follow these patterns:

- **Event-driven communication** - Services communicate through SNS topics and SQS queues rather than direct API calls where possible
- **Shared infrastructure** - Development environments share PostgreSQL and LocalStack instances to match production topology
- **Independent deployment** - Each service can be deployed independently and has its own infrastructure definitions
- **Environment flexibility** - Services can be configured to use local, staging, or production dependencies through environment variables

For more details on individual services, see their respective README files.
