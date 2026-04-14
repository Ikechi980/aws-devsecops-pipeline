#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

export DEV_LOCK_FILE="$REPO_ROOT/.dev.integrated.lock"
export DEV_LOG_DIR="$REPO_ROOT/.dev-logs/integrated"
require_cargo_lambda_version

PKI_ROOT="$REPO_ROOT/pki"
GATEWAY_ROOT="$REPO_ROOT/headend-gateway"
HEADEND_API_ROOT="$REPO_ROOT/headend-api"
CORE_CHANGE_PUBLISHER_ROOT="$REPO_ROOT/core-change-publisher"

usage() {
    echo "Usage: $0 [run|status|stop]"
    echo "  run    - Start integrated ensure stack (infra + pki + headend-gateway + headend-api + core-change-publisher)"
    echo "  status - Show lock and infrastructure status"
    echo "  stop   - Stop integrated stack and clear lock (use --force if needed)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${CORE_CHANGE_BRIDGE_PID:-}" ]]; then
        stop_process_group "$CORE_CHANGE_BRIDGE_PID"
    fi
    if [[ -n "${CORE_CHANGE_PUBLISHER_PID:-}" ]]; then
        stop_process_group "$CORE_CHANGE_PUBLISHER_PID"
    fi
    if [[ -n "${HEADEND_API_PID:-}" ]]; then
        stop_process_group "$HEADEND_API_PID"
    fi
    if [[ -n "${GATEWAY_PID:-}" ]]; then
        stop_process_group "$GATEWAY_PID"
    fi
    if [[ -n "${PKI_PID:-}" ]]; then
        stop_process_group "$PKI_PID"
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
    if [[ -f "$dir/.env.example" && ! -f "$dir/.env" ]]; then
        echo "Creating .env from .env.example in $dir..."
        cp "$dir/.env.example" "$dir/.env"
    fi
}

source_shared_env() {
    if [[ -f "$REPO_ROOT/infra/dev.env" ]]; then
        set -a
        source "$REPO_ROOT/infra/dev.env"
        set +a
    fi

    : "${AWS_ACCESS_KEY_ID:?environment variable AWS_ACCESS_KEY_ID not defined}"
    : "${AWS_SECRET_ACCESS_KEY:?environment variable AWS_SECRET_ACCESS_KEY not defined}"
    : "${AWS_REGION:?environment variable AWS_REGION not defined}"
    : "${AWS_DEFAULT_REGION:?environment variable AWS_DEFAULT_REGION not defined}"

    export AWS_ACCESS_KEY_ID
    export AWS_SECRET_ACCESS_KEY
    export AWS_REGION
    export AWS_DEFAULT_REGION

    SHARED_AWS_ENDPOINT_URL="$LOCALSTACK_ENDPOINT_URL"
    SHARED_STEP_CA_URL="https://${LOOPBACK_HOST}:9100"

    INTEGRATED_CORE_AWS_ENDPOINT_URL="http://${LOOPBACK_HOST}:4566"
    INTEGRATED_CORE_CHANGE_QUEUE_NAME="resources-events-test"
    INTEGRATED_CORE_RESOURCES_API_BASE_URL="http://${LOOPBACK_HOST}:9000/lambda-url/resources-api"
    CORE_CHANGE_PUBLISHER_LAMBDA_URL="$CORE_CHANGE_PUBLISHER_INVOKE_URL_LOCAL"
    INTEGRATED_STEP_CA_CERTS_DIR="$REPO_ROOT/infra/stepca/data/certs"
    INTEGRATED_STEP_CA_PROVISIONER_KEY_PATH="$REPO_ROOT/infra/stepca/data/secrets/provisioner.key"
    INTEGRATED_STEP_CA_PROVISIONER_KEY_ID="$(jq -r '.authority.provisioners[0].key.kid // empty' "$REPO_ROOT/infra/stepca/data/config/ca.json" 2>/dev/null || true)"

    if [[ -z "$INTEGRATED_STEP_CA_PROVISIONER_KEY_ID" ]]; then
        echo "Error: failed to resolve step-ca provisioner key ID from infra/stepca/data/config/ca.json"
        exit 1
    fi
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

wait_for_localstack_resources() {
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if docker exec ensure-localstack awslocal sqs get-queue-url --queue-name headend-test-queue >/dev/null 2>&1 \
            && docker exec ensure-localstack awslocal sqs get-queue-url --queue-name core-change-events-queue >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: ensure-cloud LocalStack resources did not become ready at $SHARED_AWS_ENDPOINT_URL"
    exit 1
}

wait_for_core_bridge_queue() {
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if docker exec sentrics-localstack awslocal sqs get-queue-url --queue-name "$INTEGRATED_CORE_CHANGE_QUEUE_NAME" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: could not resolve queue '$INTEGRATED_CORE_CHANGE_QUEUE_NAME' at $INTEGRATED_CORE_AWS_ENDPOINT_URL"
    echo "Start sentrics-core integrated stack first, then retry."
    exit 1
}

wait_for_pki() {
    local url="http://${LOOPBACK_HOST}:8080/v1/health"
    local log_file="${1:-}"
    local attempts=180
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${PKI_PID:-}" ]] && ! kill -0 "$PKI_PID" 2>/dev/null; then
            echo "Error: pki exited before becoming ready"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done

    echo "Error: pki did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

wait_for_gateway() {
    local url="http://${LOOPBACK_HOST}:3000/v1/health"
    local log_file="${1:-}"
    local attempts=180
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${GATEWAY_PID:-}" ]] && ! kill -0 "$GATEWAY_PID" 2>/dev/null; then
            echo "Error: headend-gateway exited before becoming ready"
            if [[ -n "$log_file" ]]; then
                print_log_tail "$log_file"
            fi
            exit 1
        fi
        sleep "$delay"
    done

    echo "Error: headend-gateway did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

wait_for_headend_api() {
    local url="$HEADEND_API_HEALTH_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: headend-api did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

wait_for_core_change_publisher() {
    local log_file="${1:-}"
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s -X POST "$CORE_CHANGE_PUBLISHER_LAMBDA_URL" -d '{"Records":[]}' >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: core-change-publisher did not become ready at $CORE_CHANGE_PUBLISHER_LAMBDA_URL"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

start_pki() {
    local log_file="$DEV_LOG_DIR/pki.integrated.log"
    PKI_PID=$(start_bg "$log_file" bash -c "
        cd \"$PKI_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        export AWS_ENDPOINT_URL=\"$SHARED_AWS_ENDPOINT_URL\"
        export STEP_CA_URL=\"$SHARED_STEP_CA_URL\"
        export STEP_CA_CERTS_DIR=\"$INTEGRATED_STEP_CA_CERTS_DIR\"
        export STEP_CA_PROVISIONER_KEY_PATH=\"$INTEGRATED_STEP_CA_PROVISIONER_KEY_PATH\"
        export STEP_CA_PROVISIONER_KEY_ID=\"$INTEGRATED_STEP_CA_PROVISIONER_KEY_ID\"
        cargo run
    ")
    wait_for_pki "$log_file"
}

start_gateway() {
    local log_file="$DEV_LOG_DIR/headend-gateway.integrated.log"
    GATEWAY_PID=$(start_bg "$log_file" bash -c "
        cd \"$GATEWAY_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        export AWS_ENDPOINT_URL=\"$SHARED_AWS_ENDPOINT_URL\"
        cargo run
    ")
    wait_for_gateway "$log_file"
}

start_headend_api() {
    local log_file="$DEV_LOG_DIR/headend-api.integrated.log"
    HEADEND_API_PID=$(start_bg "$log_file" bash -c "
        cd \"$HEADEND_API_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        export AWS_ENDPOINT_URL=\"$SHARED_AWS_ENDPOINT_URL\"
        export CORE_RESOURCES_API_BASE_URL=\"$INTEGRATED_CORE_RESOURCES_API_BASE_URL\"
        export ALLOW_UNAUTHENTICATED=1
        cargo lambda watch --package headend-api --bin headend-api --invoke-address \"$LOOPBACK_HOST\" --invoke-port 9202
    ")
    wait_for_headend_api "$log_file"
}

start_core_change_publisher() {
    local log_file="$DEV_LOG_DIR/core-change-publisher.integrated.log"
    CORE_CHANGE_PUBLISHER_PID=$(start_bg "$log_file" bash -c "
        cd \"$CORE_CHANGE_PUBLISHER_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        export AWS_ENDPOINT_URL=\"$SHARED_AWS_ENDPOINT_URL\"
        cargo lambda watch --package core-change-publisher --bin core-change-publisher --invoke-address \"$LOOPBACK_HOST\" --invoke-port 9201
    ")

    wait_for_core_change_publisher "$log_file"

    CORE_CHANGE_BRIDGE_PID=$(start_bg "$DEV_LOG_DIR/core-change-publisher.bridge.integrated.log" bash -c "
        cd \"$CORE_CHANGE_PUBLISHER_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        export BRIDGE_AWS_ENDPOINT_URL=\"$INTEGRATED_CORE_AWS_ENDPOINT_URL\"
        ./scripts/bridge-integrated.sh
    ")
}

monitor_background_processes() {
    while true; do
        for proc in \
            "pki:$PKI_PID" \
            "headend-gateway:$GATEWAY_PID" \
            "headend-api:$HEADEND_API_PID" \
            "core-change-publisher:$CORE_CHANGE_PUBLISHER_PID" \
            "core-change-publisher-bridge:$CORE_CHANGE_BRIDGE_PID"; do
            local name="${proc%%:*}"
            local pid="${proc##*:}"
            if ! kill -0 "$pid" 2>/dev/null; then
                echo "Error: $name process exited unexpectedly (pid $pid)."
                return 1
            fi
        done
        sleep 2
    done
}

run() {
    acquire_lock "integrated ensure run"
    trap cleanup EXIT INT TERM

    check_docker
    require_ports_free 4666 8080 8081 8082 8443 9100 9201 9202 27017 3000
    ensure_dev_ca
    ensure_nginx_server_cert
    infra_up

    ensure_env_file "$PKI_ROOT"
    ensure_env_file "$GATEWAY_ROOT"
    ensure_env_file "$HEADEND_API_ROOT"
    ensure_env_file "$CORE_CHANGE_PUBLISHER_ROOT"

    source_shared_env
    wait_for_localstack_resources
    wait_for_core_bridge_queue
    start_infra_logs

    echo "Starting pki..."
    start_pki

    echo "Starting headend-gateway..."
    start_gateway

    echo "Starting headend-api..."
    start_headend_api

    echo "Starting core-change-publisher..."
    start_core_change_publisher

    echo "Integrated ensure stack is running."
    echo "WebSocket mTLS endpoint: wss://localhost:8443/gateway/v1/ws"
    echo "API mTLS endpoint:       https://localhost:8443/api/v1"
    echo "Logs:"
    echo "  $DEV_LOG_DIR/pki.integrated.log"
    echo "  $DEV_LOG_DIR/headend-gateway.integrated.log"
    echo "  $DEV_LOG_DIR/headend-api.integrated.log"
    echo "  $DEV_LOG_DIR/core-change-publisher.integrated.log"
    echo "  $DEV_LOG_DIR/core-change-publisher.bridge.integrated.log"
    echo "Press Ctrl-C to stop."

    monitor_background_processes
}

status() {
    lock_info
    if infra_running; then
        echo "Infrastructure: running"
    else
        echo "Infrastructure: stopped"
    fi
}

stop() {
    if [[ -f "$DEV_LOCK_FILE" ]]; then
        IFS='|' read -r lock_pid lock_cmd < "$DEV_LOCK_FILE" || true
        if [[ -n "${lock_pid:-}" ]] && kill -0 "$lock_pid" 2>/dev/null; then
            if [[ "${2:-}" != "--force" ]]; then
                echo "Error: integrated stack is running (pid $lock_pid): $lock_cmd"
                echo "Use Ctrl-C in that terminal or re-run with --force."
                exit 1
            fi
            kill -TERM "$lock_pid" 2>/dev/null || true
            sleep 1
        fi
    fi

    infra_down
    rm -f "$DEV_LOCK_FILE"
    echo "✓ Integrated ensure stack stopped"
}

case "${1:-}" in
    run)    run ;;
    status) status ;;
    stop)   stop "$@" ;;
    *)      usage ;;
esac
