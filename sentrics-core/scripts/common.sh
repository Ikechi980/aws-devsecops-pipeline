#!/usr/bin/env bash

# Shared utilities for development scripts

if [[ -z "${REPO_ROOT:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    export REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
fi
export INFRA_DIR="$REPO_ROOT/infra"
export COMPOSE_FILE="$INFRA_DIR/docker-compose.yml"
export DEV_LOCK_FILE="$REPO_ROOT/.dev.lock"
export DEV_LOG_DIR="$REPO_ROOT/.dev-logs"
# Fixed compose project name for local development.
export COMPOSE_PROJECT_NAME="sentrics_core"
export LOOPBACK_HOST="127.0.0.1"
export LOCALSTACK_ENDPOINT_URL="http://${LOOPBACK_HOST}:4566"
export RESOURCES_API_BASE_URL_LOCAL="http://${LOOPBACK_HOST}:9000/lambda-url/resources-api/v1"
export RESOURCES_API_HEALTH_URL_LOCAL="${RESOURCES_API_BASE_URL_LOCAL}/health"
export CHANGE_LOGGER_INVOKE_URL_LOCAL="http://${LOOPBACK_HOST}:9001/2015-03-31/functions/resources-change-logger/invocations"

print_header() {
    echo ""
    echo "=========================================="
    echo "$1"
    echo "=========================================="
    echo ""
}

check_docker() {
    if ! command -v docker &> /dev/null; then
        echo "Error: Docker is not installed or not in PATH"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        echo "Error: Docker daemon is not running"
        exit 1
    fi
}

lock_info() {
    if [[ -f "$DEV_LOCK_FILE" ]]; then
        IFS='|' read -r lock_pid lock_cmd < "$DEV_LOCK_FILE" || true
        if [[ -n "${lock_pid:-}" ]]; then
            echo "Lock: pid=$lock_pid cmd=$lock_cmd"
            return 0
        fi
    fi
    echo "Lock: none"
}

acquire_lock() {
    local cmd="$1"
    if [[ -f "$DEV_LOCK_FILE" ]]; then
        IFS='|' read -r lock_pid lock_cmd < "$DEV_LOCK_FILE" || true
        if [[ -n "${lock_pid:-}" ]] && kill -0 "$lock_pid" 2>/dev/null; then
            echo "Error: dev environment already running (pid $lock_pid): $lock_cmd"
            exit 1
        fi
        rm -f "$DEV_LOCK_FILE"
    fi
    echo "$$|$cmd" > "$DEV_LOCK_FILE"
}

release_lock() {
    if [[ -f "$DEV_LOCK_FILE" ]]; then
        IFS='|' read -r lock_pid _ < "$DEV_LOCK_FILE" || true
        if [[ "${lock_pid:-}" == "$$" ]]; then
            rm -f "$DEV_LOCK_FILE"
        fi
    fi
}

infra_running() {
    docker ps | grep -q sentrics-postgres && docker ps | grep -q sentrics-localstack
}

infra_up() {
    check_docker
    docker compose -f "$COMPOSE_FILE" up -d --wait
}

require_ports_free() {
    local ports=("$@")
    local have_checker=0

    if command -v lsof >/dev/null 2>&1; then
        have_checker=1
        for port in "${ports[@]}"; do
            local listeners
            listeners=$(lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | tail -n +2 || true)
            if [[ -n "$listeners" ]]; then
                echo "Error: port $port is already in use."
                echo "Listening processes:"
                echo "$listeners"
                echo "Stop the process using the port or change the service port."
                exit 1
            fi
        done
        return 0
    fi

    if command -v ss >/dev/null 2>&1; then
        have_checker=1
        for port in "${ports[@]}"; do
            local listeners
            listeners=$(ss -ltnp "sport = :$port" 2>/dev/null | tail -n +2 || true)
            if [[ -n "$listeners" ]]; then
                echo "Error: port $port is already in use."
                echo "Listening processes:"
                echo "$listeners"
                echo "Stop the process using the port or change the service port."
                exit 1
            fi
        done
        return 0
    fi

    if command -v netstat >/dev/null 2>&1; then
        have_checker=1
        for port in "${ports[@]}"; do
            local listeners
            listeners=$(netstat -ltnp 2>/dev/null | awk -v port=":$port" '$4 ~ port {print}' || true)
            if [[ -n "$listeners" ]]; then
                echo "Error: port $port is already in use."
                echo "Listening processes:"
                echo "$listeners"
                echo "Stop the process using the port or change the service port."
                exit 1
            fi
        done
        return 0
    fi

    if [[ "$have_checker" -eq 0 ]]; then
        echo "Warning: cannot check for free ports (missing lsof/ss/netstat)."
    fi
}

wait_for_localstack_init() {
    local attempts=30
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if docker exec sentrics-localstack awslocal sqs get-queue-url --queue-name yardi-sync-events >/dev/null 2>&1 \
            && docker exec sentrics-localstack awslocal sqs get-queue-url --queue-name resources-events-test >/dev/null 2>&1 \
            && docker exec sentrics-localstack awslocal sqs get-queue-url --queue-name resources-change-logger-events >/dev/null 2>&1 \
            && docker exec sentrics-localstack awslocal dynamodb describe-table --table-name resources-change-log >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: LocalStack SQS queues not ready after ${attempts}s"
    return 1
}

infra_down() {
    check_docker
    docker compose -f "$COMPOSE_FILE" down
}

infra_reset() {
    check_docker
    docker compose -f "$COMPOSE_FILE" down -v
}

tail_infra_logs() {
    check_docker
    docker compose -f "$COMPOSE_FILE" logs -f
}

start_bg() {
    local log_file="$1"
    shift
    mkdir -p "$DEV_LOG_DIR"
    if command -v setsid >/dev/null 2>&1; then
        setsid "$@" >"$log_file" 2>&1 &
    else
        "$@" >"$log_file" 2>&1 &
    fi
    echo $!
}

stop_process_group() {
    local pid="${1:-}"
    if [[ -z "$pid" ]]; then
        return 0
    fi
    kill -TERM -"$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
}

print_log_tail() {
    local log_file="$1"
    local lines="${2:-40}"

    if [[ ! -f "$log_file" ]]; then
        echo "Log file not found: $log_file"
        return 0
    fi

    echo "Last ${lines} lines from $log_file:"
    tail -n "$lines" "$log_file"
}

expected_cargo_lambda_version() {
    "$REPO_ROOT/scripts/ci/cargo-lambda-version.sh"
}

require_cargo_lambda_version() {
    local expected_version
    local actual_version

    if ! command -v cargo >/dev/null 2>&1; then
        echo "Error: cargo is not installed or not in PATH"
        exit 1
    fi

    if ! cargo lambda --version >/dev/null 2>&1; then
        expected_version="$(expected_cargo_lambda_version)"
        echo "Error: cargo-lambda is not installed or not available via 'cargo lambda'"
        echo "Install it with:"
        echo "  cargo install cargo-lambda --locked --version \"$expected_version\""
        exit 1
    fi

    expected_version="$(expected_cargo_lambda_version)"
    actual_version="$(cargo lambda --version 2>/dev/null || true)"

    if [[ "$actual_version" != *" ${expected_version} "* ]]; then
        echo "Error: cargo-lambda ${expected_version} is required for local development"
        echo "Found: ${actual_version:-unknown}"
        echo "Install the required version with:"
        echo "  cargo install cargo-lambda --locked --version \"$expected_version\""
        exit 1
    fi
}
