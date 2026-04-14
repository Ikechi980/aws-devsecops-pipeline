#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

export DEV_LOCK_FILE="$REPO_ROOT/.dev.integrated.lock"
export DEV_LOG_DIR="$REPO_ROOT/.dev-logs/integrated"
require_cargo_lambda_version

RESOURCES_API_ROOT="$REPO_ROOT/resources-api"
CHANGE_LOGGER_ROOT="$REPO_ROOT/resources-change-logger"
YARDI_SYNC_ROOT="$REPO_ROOT/yardi-sync"

usage() {
    echo "Usage: $0 [run|status|stop]"
    echo "  run    - Start integrated core stack (infra + resources-api + resources-change-logger + yardi-sync)"
    echo "  status - Show lock and infrastructure status"
    echo "  stop   - Stop integrated stack and clear lock (use --force if needed)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ -n "${YARDI_PID:-}" ]]; then
        stop_process_group "$YARDI_PID"
    fi
    if [[ -n "${CHANGE_LOGGER_PID:-}" ]]; then
        stop_process_group "$CHANGE_LOGGER_PID"
    fi
    if [[ -n "${CHANGE_LOGGER_BRIDGE_PID:-}" ]]; then
        stop_process_group "$CHANGE_LOGGER_BRIDGE_PID"
    fi
    if [[ -n "${RESOURCES_API_PID:-}" ]]; then
        stop_process_group "$RESOURCES_API_PID"
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
}

run_migrations() {
    echo "Running resources-api migrations..."
    (
        cd "$RESOURCES_API_ROOT"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        MIGRATE_PORT=9010 ./scripts/migrate.sh
    )
}

start_infra_logs() {
    tail_infra_logs &
    LOGS_PID=$!
}

wait_for_resources_api() {
    local url="$RESOURCES_API_HEALTH_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: resources-api did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

wait_for_change_logger() {
    local url="$CHANGE_LOGGER_INVOKE_URL_LOCAL"
    local log_file="${1:-}"
    local attempts=40
    local delay=1

    for _ in $(seq 1 "$attempts"); do
        if curl -s -X POST "$url" -d '{"Records":[]}' >/dev/null 2>&1; then
            return 0
        fi
        sleep "$delay"
    done

    echo "Error: resources-change-logger did not become ready at $url"
    if [[ -n "$log_file" ]]; then
        print_log_tail "$log_file"
    fi
    exit 1
}

start_resources_api() {
    local log_file="$DEV_LOG_DIR/resources-api.integrated.log"
    RESOURCES_API_PID=$(start_bg "$log_file" bash -c "
        cd \"$RESOURCES_API_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        cargo lambda watch -p resources-api --bin resources-api --invoke-address \"$LOOPBACK_HOST\"
    ")
    wait_for_resources_api "$log_file"
}

start_change_logger() {
    CHANGE_LOGGER_BRIDGE_PID=$(start_bg "$DEV_LOG_DIR/resources-change-logger.bridge.integrated.log" bash -c "
        cd \"$CHANGE_LOGGER_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        ./scripts/bridge.sh
    ")

    local log_file="$DEV_LOG_DIR/resources-change-logger.integrated.log"
    CHANGE_LOGGER_PID=$(start_bg "$log_file" bash -c "
        cd \"$CHANGE_LOGGER_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        cargo lambda watch -p resources-change-logger --bin resources-change-logger --invoke-address \"$LOOPBACK_HOST\" --invoke-port 9001
    ")

    wait_for_change_logger "$log_file"
}

start_yardi_sync() {
    YARDI_PID=$(start_bg "$DEV_LOG_DIR/yardi-sync.integrated.log" bash -c "
        cd \"$YARDI_SYNC_ROOT\"
        if [[ -f .env ]]; then
            set -a
            source .env
            set +a
        fi
        cargo run
    ")
}

monitor_background_processes() {
    while true; do
        for proc in \
            "resources-api:$RESOURCES_API_PID" \
            "resources-change-logger:$CHANGE_LOGGER_PID" \
            "resources-change-logger-bridge:$CHANGE_LOGGER_BRIDGE_PID" \
            "yardi-sync:$YARDI_PID"; do
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
    acquire_lock "integrated core run"
    trap cleanup EXIT INT TERM

    check_docker
    require_ports_free 4566 5432 3001 9000 9001
    infra_up
    wait_for_localstack_init

    ensure_env_file "$RESOURCES_API_ROOT"
    ensure_env_file "$CHANGE_LOGGER_ROOT"
    ensure_env_file "$YARDI_SYNC_ROOT"

    source_shared_env
    run_migrations
    start_infra_logs

    echo "Starting resources-api..."
    start_resources_api

    echo "Starting resources-change-logger..."
    start_change_logger

    echo "Starting yardi-sync..."
    start_yardi_sync

    echo "Integrated core stack is running."
    echo "Logs:"
    echo "  $DEV_LOG_DIR/resources-api.integrated.log"
    echo "  $DEV_LOG_DIR/resources-change-logger.integrated.log"
    echo "  $DEV_LOG_DIR/resources-change-logger.bridge.integrated.log"
    echo "  $DEV_LOG_DIR/yardi-sync.integrated.log"
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
    echo "✓ Integrated core stack stopped"
}

case "${1:-}" in
    run)    run ;;
    status) status ;;
    stop)   stop "$@" ;;
    *)      usage ;;
esac
