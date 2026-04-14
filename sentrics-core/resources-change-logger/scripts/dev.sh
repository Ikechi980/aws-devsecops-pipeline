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
    echo "  test - Start infra, run tests, then tear down"
    echo ""
    echo "Options:"
    echo "  --reset - Reset infra before starting (run/test/deps)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${BRIDGE_PID:-}" ]]; then
        stop_process_group "$BRIDGE_PID"
    fi
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
    if [[ ! -f "$PROJECT_ROOT/.env" ]]; then
        echo "Creating .env from .env.example..."
        cp "$PROJECT_ROOT/.env.example" "$PROJECT_ROOT/.env"
    fi
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

prebuild_artifacts() {
    echo "Prebuilding run binary..."
    (cd "$PROJECT_ROOT" && cargo build --bin resources-change-logger)

    echo "Prebuilding test artifacts..."
    (cd "$PROJECT_ROOT" && cargo test --no-run)
}

aws_local() {
    aws --region "${AWS_REGION:-us-east-1}" --endpoint-url "$AWS_ENDPOINT_URL" "$@"
}

wait_for_localstack_resources() {
    local attempts=30
    local delay=1
    local table_name="${CHANGE_LOG_TABLE_NAME:-}"

    if [[ -z "$table_name" ]]; then
        echo "Error: CHANGE_LOG_TABLE_NAME must be set"
        exit 1
    fi

    for _ in $(seq 1 "$attempts"); do
        if aws_local sqs get-queue-url --queue-name resources-change-logger-events >/dev/null 2>&1 \
            && aws_local dynamodb describe-table --table-name "$table_name" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: LocalStack resources did not become ready"
    exit 1
}

wait_for_lambda() {
    local url="$CHANGE_LOGGER_INVOKE_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=30
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s -X POST "$url" -d '{"Records":[]}' >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${LAMBDA_PID:-}" ]] && ! kill -0 "$LAMBDA_PID" 2>/dev/null; then
            echo "Error: resources-change-logger Lambda exited before becoming ready"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done

    echo "Error: resources-change-logger Lambda did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

deps() {
    local reset_flag="${2:-}"
    acquire_lock "resources-change-logger deps"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566
    infra_up
    wait_for_localstack_init
    ensure_env_file

    echo "Dependencies are running. Tailing logs (Ctrl-C to stop)..."
    start_infra_logs
    wait "$LOGS_PID"
}

run() {
    local reset_flag="${2:-}"
    acquire_lock "resources-change-logger run"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566 9001
    infra_up
    wait_for_localstack_init
    ensure_env_file
    wait_for_localstack_resources
    start_infra_logs

    echo "Starting resources-change-logger Lambda locally (Ctrl-C to stop)..."
    BRIDGE_PID=$(start_bg "$DEV_LOG_DIR/resources-change-logger.bridge.log" bash -c "cd \"$PROJECT_ROOT\" && ./scripts/bridge.sh")
    cd "$PROJECT_ROOT"
    cargo lambda watch -p resources-change-logger --bin resources-change-logger --invoke-address "$LOOPBACK_HOST" --invoke-port 9001
}

test() {
    local reset_flag="${2:-}"
    acquire_lock "resources-change-logger test"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    require_ports_free 4566
    infra_up
    wait_for_localstack_init
    ensure_env_file
    wait_for_localstack_resources

    prebuild_artifacts

    echo "Starting resources-change-logger Lambda for tests..."
    local log_file="$DEV_LOG_DIR/resources-change-logger.test.log"
    LAMBDA_PID=$(start_bg "$log_file" bash -c "cd \"$PROJECT_ROOT\" && cargo lambda watch -p resources-change-logger --bin resources-change-logger --invoke-address \"$LOOPBACK_HOST\" --invoke-port 9001")
    wait_for_lambda "$log_file"

    echo "Running tests..."
    (cd "$PROJECT_ROOT" && cargo test -- --nocapture)
}

case "${1:-}" in
    deps) deps "$@" ;;
    run)  run "$@" ;;
    test) test "$@" ;;
    *)    usage ;;
esac
