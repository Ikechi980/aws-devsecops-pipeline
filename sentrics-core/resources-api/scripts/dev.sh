#!/bin/bash
set -e

# Source common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

PROJECT_ROOT="$SCRIPT_DIR/.."
require_cargo_lambda_version

usage() {
    echo "Usage: $0 [deps|run|test|migrate]"
    echo "  deps    - Start infra and run migrations, then tail infra logs"
    echo "  run     - Start infra, run migrations, run resources-api"
    echo "  test    - Start infra, run resources-api, run tests, then tear down"
    echo "  migrate - Run database migrations (infra must be running)"
    echo ""
    echo "Options:"
    echo "  --reset - Reset infra before starting (run/test/deps)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${API_PID:-}" ]]; then
        stop_process_group "$API_PID"
    fi
    if [[ -n "${LOGS_PID:-}" ]]; then
        kill "$LOGS_PID" 2>/dev/null || true
    fi
    infra_down || true
    release_lock || true
    exit $status
}

ensure_env_file() {
    if [[ ! -f "$PROJECT_ROOT/.env" ]]; then
        echo "Creating .env from .env.example..."
        cp "$PROJECT_ROOT/.env.example" "$PROJECT_ROOT/.env"
    fi
}

run_migrations() {
    echo "Running database migrations..."
    (cd "$PROJECT_ROOT" && MIGRATE_PORT=9010 ./scripts/migrate.sh)
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

prebuild_artifacts() {
    echo "Prebuilding run binary..."
    (cd "$PROJECT_ROOT" && cargo build --bin resources-api)

    echo "Prebuilding test artifacts..."
    (cd "$PROJECT_ROOT" && cargo test --no-run)
}

wait_for_health() {
    local url="$RESOURCES_API_HEALTH_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=30
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${API_PID:-}" ]] && ! kill -0 "$API_PID" 2>/dev/null; then
            echo "Error: resources-api exited before becoming ready"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done

    echo "Error: resources-api did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

deps() {
    local reset_flag="${2:-}"
    acquire_lock "resources-api deps"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 9000
    infra_up
    wait_for_localstack_init
    ensure_env_file
    run_migrations
    echo "Tailing infra logs (Ctrl-C to stop)..."
    tail_infra_logs
}

run() {
    local reset_flag="${2:-}"
    acquire_lock "resources-api run"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 9000
    infra_up
    wait_for_localstack_init
    ensure_env_file
    run_migrations
    start_infra_logs

    echo "Starting resources-api (Ctrl-C to stop)..."
    cd "$PROJECT_ROOT"
    cargo lambda watch -p resources-api --bin resources-api --invoke-address "$LOOPBACK_HOST"
}

test() {
    local reset_flag="${2:-}"
    acquire_lock "resources-api test"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 9000
    infra_up
    wait_for_localstack_init
    ensure_env_file
    run_migrations

    prebuild_artifacts

    echo "Starting resources-api for tests..."
    local log_file="$DEV_LOG_DIR/resources-api.test.log"
    API_PID=$(start_bg "$log_file" bash -c "cd \"$PROJECT_ROOT\" && cargo lambda watch -p resources-api --bin resources-api --invoke-address \"$LOOPBACK_HOST\"")

    wait_for_health "$log_file"

    echo "Running tests..."
    (cd "$PROJECT_ROOT" && cargo test)
}

migrate() {
    if ! infra_running; then
        echo "Error: infrastructure is not running. Use ./scripts/dev.sh run or deps first."
        exit 1
    fi
    run_migrations
}

case "${1:-}" in
    deps)    deps "$@" ;;
    run)     run "$@" ;;
    test)    test "$@" ;;
    migrate) migrate ;;
    *)       usage ;;
esac
