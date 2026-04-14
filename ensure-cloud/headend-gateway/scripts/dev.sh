#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

PROJECT_ROOT="$SCRIPT_DIR/.."
PKI_ROOT="$REPO_ROOT/pki"

usage() {
    echo "Usage: $0 [deps|run|test]"
    echo "  deps - Start infra and pki, then tail logs"
    echo "  run  - Start infra and pki, then run headend-gateway"
    echo "  test - Start infra and pki, run headend-gateway, run tests, then tear down"
    echo ""
    echo "Options:"
    echo "  --reset - Reset infra before starting (run/test/deps)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
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
    (cd "$PROJECT_ROOT" && cargo build --bin headend-gateway)

    echo "Prebuilding test artifacts..."
    (cd "$PROJECT_ROOT" && cargo test --no-run)
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

start_pki() {
    local log_file="${1:-}"
    ensure_env_file "$PKI_ROOT"
    if [[ -n "$log_file" ]]; then
        PKI_PID=$(start_bg "$log_file" bash -c "cd \"$PKI_ROOT\" && cargo run")
    else
        (cd "$PKI_ROOT" && cargo run) &
        PKI_PID=$!
    fi
    wait_for_pki "$log_file"
}

deps() {
    local reset_flag="${2:-}"
    acquire_lock "headend-gateway deps"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8080 8081 8443 9100 3000
    ensure_dev_ca
    ensure_nginx_server_cert
    infra_up
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"
    start_infra_logs
    start_pki

    echo "Dependencies are running. Tailing logs (Ctrl-C to stop)..."
    wait "$PKI_PID"
}

run() {
    local reset_flag="${2:-}"
    acquire_lock "headend-gateway run"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8080 8081 8443 9100 3000
    ensure_dev_ca
    ensure_nginx_server_cert
    infra_up
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"
    start_infra_logs
    start_pki

    echo "Starting headend-gateway (Ctrl-C to stop)..."
    cd "$PROJECT_ROOT"
    cargo run
}

test() {
    local reset_flag="${2:-}"
    acquire_lock "headend-gateway test"
    trap cleanup EXIT INT TERM

    if [[ "$reset_flag" == "--reset" ]]; then
        infra_reset
    fi

    check_docker
    require_ports_free 4666 8080 8081 8443 9100 3000
    ensure_dev_ca
    ensure_nginx_server_cert
    infra_up
    wait_for_ensure_localstack_resources
    ensure_env_file "$PROJECT_ROOT"

    prebuild_artifacts

    echo "Starting pki for tests..."
    start_pki "$DEV_LOG_DIR/pki.test.log"

    echo "Starting headend-gateway for tests..."
    local log_file="$DEV_LOG_DIR/headend-gateway.test.log"
    GATEWAY_PID=$(start_bg "$log_file" bash -c "cd \"$PROJECT_ROOT\" && cargo run")

    wait_for_gateway "$log_file"

    echo "Running tests..."
    (cd "$PROJECT_ROOT" && cargo test)
}

case "${1:-}" in
    deps) deps "$@" ;;
    run)  run "$@" ;;
    test) test "$@" ;;
    *)    usage ;;
esac
