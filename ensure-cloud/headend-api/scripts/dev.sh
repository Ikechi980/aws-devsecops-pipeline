#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

PROJECT_ROOT="$SCRIPT_DIR/.."
require_cargo_lambda_version

usage() {
    echo "Usage: $0 [deps|run|test]"
    echo "  deps - Start infra and tail logs"
    echo "  run  - Start infra, then run lambda locally"
    echo "  test - Start infra, run lambda locally, run tests, then tear down"
    echo ""
    echo "Options:"
    echo "  --reset - Reset infra before starting (run/test/deps)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${LAMBDA_PID:-}" ]]; then
        stop_process_group "$LAMBDA_PID"
    fi
    if [[ -n "${LOGS_PID:-}" ]]; then
        kill "$LOGS_PID" 2>/dev/null || true
    fi
    infra_down || true
    release_lock || true
    exit $status
}

ensure_env_file() {
    local dir="$1"
    if [[ ! -f "$dir/.env" ]]; then
        echo "Creating .env from .env.example in $dir..."
        cp "$dir/.env.example" "$dir/.env"
    fi
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

prebuild_artifacts() {
    echo "Prebuilding run binary..."
    (cd "$PROJECT_ROOT" && cargo build --bin headend-api)

    echo "Prebuilding test artifacts..."
    (cd "$PROJECT_ROOT" && cargo test --no-run)
}

wait_for_lambda() {
    local url="$HEADEND_API_HEALTH_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=30
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${LAMBDA_PID:-}" ]] && ! kill -0 "$LAMBDA_PID" 2>/dev/null; then
            echo "Error: headend-api Lambda exited before becoming ready"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done

    echo "Error: headend-api Lambda did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

deps() {
    local reset_flag="${2:-}"
    acquire_lock "headend-api deps"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8081 8082 8443 9100 9202
    ensure_dev_ca
    ensure_nginx_server_cert
    docker compose -f "$COMPOSE_FILE" build mock-systems-api mock-core-resources-api
    docker compose -f "$COMPOSE_FILE" up -d --wait
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"

    echo "Dependencies are running. Tailing logs (Ctrl-C to stop)..."
    start_infra_logs
    wait "$LOGS_PID"
}

run() {
    local reset_flag="${2:-}"
    acquire_lock "headend-api run"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8081 8082 8443 9100 9202 27017
    ensure_dev_ca
    ensure_nginx_server_cert
    docker compose -f "$COMPOSE_FILE" build mock-systems-api mock-core-resources-api
    docker compose -f "$COMPOSE_FILE" up -d --wait
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"
    start_infra_logs

    echo "Starting headend-api Lambda locally (Ctrl-C to stop)..."
    cd "$PROJECT_ROOT"
    cargo lambda watch --package headend-api --bin headend-api --invoke-address "$LOOPBACK_HOST" --invoke-port 9202
}

test() {
    local reset_flag="${2:-}"
    acquire_lock "headend-api test"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8081 8082 8443 9100 9202
    ensure_dev_ca
    ensure_nginx_server_cert
    docker compose -f "$COMPOSE_FILE" build mock-systems-api mock-core-resources-api
    docker compose -f "$COMPOSE_FILE" up -d --wait
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"

    prebuild_artifacts

    echo "Starting headend-api Lambda for tests..."
    local log_file="$DEV_LOG_DIR/headend-api.test.log"
    LAMBDA_PID=$(start_bg "$log_file" bash -c "cd \"$PROJECT_ROOT\" && cargo lambda watch --package headend-api --bin headend-api --invoke-address \"$LOOPBACK_HOST\" --invoke-port 9202")
    wait_for_lambda "$log_file"

    echo "Running tests..."
    (cd "$PROJECT_ROOT" && cargo test)
}

case "${1:-}" in
    deps) deps "$@" ;;
    run)  run "$@" ;;
    test) test "$@" ;;
    *)    usage ;;
esac
