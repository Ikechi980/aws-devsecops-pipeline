#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

PROJECT_ROOT="$SCRIPT_DIR/.."
require_cargo_lambda_version

usage() {
    echo "Usage: $0 [deps|run|test]"
    echo "  deps - Start infra and resources-api, then tail logs"
    echo "  run  - Start infra, resources-api, and yardi-sync"
    echo "  test - Start infra, resources-api, yardi-sync, run tests, then tear down"
    echo ""
    echo "Options:"
    echo "  --reset - Reset infra before starting (run/test/deps)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${YARDI_PID:-}" ]]; then
        stop_process_group "$YARDI_PID"
    fi
    if [[ -n "${RESOURCES_PID:-}" ]]; then
        stop_process_group "$RESOURCES_PID"
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
        echo "Created .env file. Update credentials before running full sync."
    fi
}

ensure_resources_api_env_file() {
    if [[ ! -f "$REPO_ROOT/resources-api/.env" ]]; then
        echo "Creating resources-api .env from .env.example..."
        cp "$REPO_ROOT/resources-api/.env.example" "$REPO_ROOT/resources-api/.env"
    fi
}

run_migrations() {
    (cd "$REPO_ROOT/resources-api" && MIGRATE_PORT=9010 ./scripts/migrate.sh)
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

prebuild_artifacts() {
    echo "Prebuilding resources-api run binary..."
    (
        cd "$REPO_ROOT/resources-api" &&
            cargo build --bin resources-api
    )

    echo "Prebuilding yardi-sync run binary..."
    (cd "$PROJECT_ROOT" && cargo build --bin yardi-sync)

    echo "Prebuilding test artifacts..."
    (cd "$PROJECT_ROOT" && cargo test --no-run)
}

build_mock_yardi_api() {
    docker compose -f "$COMPOSE_FILE" build mock-yardi-api
}

wait_for_resources_api() {
    local url="$RESOURCES_API_HEALTH_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=30
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${RESOURCES_PID:-}" ]] && ! kill -0 "$RESOURCES_PID" 2>/dev/null; then
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

start_resources_api() {
    local log_file="${1:-}"
    ensure_resources_api_env_file
    run_migrations
    if [[ -n "$log_file" ]]; then
        RESOURCES_PID=$(start_bg "$log_file" bash -c "cd \"$REPO_ROOT/resources-api\" && cargo lambda watch -p resources-api --bin resources-api --invoke-address \"$LOOPBACK_HOST\"")
    else
        (cd "$REPO_ROOT/resources-api" && cargo lambda watch -p resources-api --bin resources-api --invoke-address "$LOOPBACK_HOST") &
        RESOURCES_PID=$!
    fi
    wait_for_resources_api "$log_file"
}

wait_for_yardi_startup() {
    local log_file="${1:-}"
    local attempts=3
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if [[ -n "${YARDI_PID:-}" ]] && ! kill -0 "$YARDI_PID" 2>/dev/null; then
            echo "Error: yardi-sync exited during startup"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done
}

deps() {
    local reset_flag="${2:-}"
    acquire_lock "yardi-sync deps"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 3001 9000
    build_mock_yardi_api
    infra_up
    wait_for_localstack_init
    ensure_env_file
    start_infra_logs
    start_resources_api

    echo "Dependencies are running. Tailing logs (Ctrl-C to stop)..."
    wait "$RESOURCES_PID"
}

run() {
    local reset_flag="${2:-}"
    acquire_lock "yardi-sync run"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 3001 9000
    build_mock_yardi_api
    infra_up
    wait_for_localstack_init
    ensure_env_file
    start_infra_logs
    start_resources_api

    echo "Starting yardi-sync (Ctrl-C to stop)..."
    cd "$PROJECT_ROOT"
    if [[ -f .env ]]; then
        set -a
        source .env
        set +a
    fi
    cargo run
}

test() {
    local reset_flag="${2:-}"
    acquire_lock "yardi-sync test"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 5432 3001 9000
    build_mock_yardi_api
    infra_up
    wait_for_localstack_init
    ensure_env_file
    prebuild_artifacts
    start_resources_api "$DEV_LOG_DIR/resources-api.test.log"

    echo "Starting yardi-sync for tests..."
    local log_file="$DEV_LOG_DIR/yardi-sync.test.log"
    YARDI_PID=$(start_bg "$log_file" bash -c "cd \"$PROJECT_ROOT\" && cargo run")

    wait_for_yardi_startup "$log_file"

    echo "Running tests..."
    (cd "$PROJECT_ROOT" && RUST_LOG=debug cargo test -- --nocapture)
}

case "${1:-}" in
    deps) deps "$@" ;;
    run)  run "$@" ;;
    test) test "$@" ;;
    *)    usage ;;
esac
