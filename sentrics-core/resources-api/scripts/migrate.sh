#!/usr/bin/env bash
# Script to run database migrations
# This can be used locally or in CI/CD to apply migrations to a target database

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
source "$SCRIPT_DIR/common.sh"
cd "$SCRIPT_DIR/.."
require_cargo_lambda_version

# Load .env if it exists (for local dev)
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

# Use a dedicated invoke port to avoid collisions with other local lambdas.
: "${MIGRATE_PORT:?Error: MIGRATE_PORT environment variable is not set}"

# Check if DATABASE_URL is set
if [ -z "${DATABASE_URL:-}" ]; then
    echo "Error: DATABASE_URL environment variable is not set"
    echo "Usage: MIGRATE_PORT=9010 DATABASE_URL=postgres://user:pass@host/db ./scripts/migrate.sh"
    exit 1
fi

echo "Running migrations against: ${DATABASE_URL%%@*}@***"

# Start cargo lambda watch for the migrate binary in the background
echo "Starting Lambda runtime emulator on port ${MIGRATE_PORT}..."
mkdir -p "$DEV_LOG_DIR"
WATCH_LOG_FILE="$DEV_LOG_DIR/resources-api.migrate.log"
RUST_LOG=info cargo lambda watch --package resources-api --bin migrate --invoke-address "${LOOPBACK_HOST}" --invoke-port "${MIGRATE_PORT}" >"$WATCH_LOG_FILE" 2>&1 &
WATCH_PID=$!

# Cleanup function to stop the watch process
cleanup() {
    echo "Stopping Lambda runtime emulator..."
    stop_process_group "$WATCH_PID"
    wait $WATCH_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for the Lambda runtime to be ready
echo "Waiting for Lambda runtime to be ready..."
for i in {1..30}; do
    if curl -s "http://${LOOPBACK_HOST}:${MIGRATE_PORT}/_lambda/health" &>/dev/null; then
        echo "Lambda runtime is ready"
        break
    fi
    if ! kill -0 "$WATCH_PID" 2>/dev/null; then
        echo "Error: Lambda runtime exited before becoming ready"
        print_log_tail "$WATCH_LOG_FILE"
        exit 1
    fi
    if [ $i -eq 30 ]; then
        echo "Error: Lambda runtime failed to start"
        print_log_tail "$WATCH_LOG_FILE"
        exit 1
    fi
    sleep 1
done

# Invoke the migrate function
echo "Invoking migrate function..."
RUST_LOG=info cargo lambda invoke --invoke-address "${LOOPBACK_HOST}" --invoke-port "${MIGRATE_PORT}" migrate --data-ascii '{}'

echo "Migrations completed successfully!"
