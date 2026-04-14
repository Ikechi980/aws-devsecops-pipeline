#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
    echo "Usage: $0 [run|reset|status|stop]"
    echo "  run    - Start shared development infrastructure and tail logs"
    echo "  reset  - Wipe all shared data (only when nothing is running)"
    echo "  status - Show lock and infrastructure status"
    echo "  stop   - Stop infrastructure and clear lock (use --force if needed)"
    exit 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    infra_down || true
    release_lock || true
    exit $status
}

run() {
    print_header "Starting shared development infrastructure"
    acquire_lock "infra run"
    trap cleanup EXIT INT TERM

    check_docker
    require_ports_free 4666 8081 8082 8443 9100 27017
    ensure_dev_ca
    ensure_nginx_server_cert
    infra_up

    echo "Infrastructure is ready. Tailing logs (Ctrl-C to stop)..."
    tail_infra_logs
}

reset() {
    if [[ -f "$DEV_LOCK_FILE" ]]; then
        echo "Error: dev environment is running. Stop it before resetting."
        lock_info
        exit 1
    fi

    print_header "Resetting development infrastructure"
    echo "This will delete all Step CA material, nginx certs, and LocalStack state!"
    read -p "Are you sure? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Reset cancelled"
        exit 0
    fi

    infra_reset
    rm -rf "$INFRA_DIR/stepca/data" "$INFRA_DIR/nginx/certs"
    echo "✓ Infrastructure reset complete"
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
                echo "Error: dev environment is running (pid $lock_pid): $lock_cmd"
                echo "Use Ctrl-C in that terminal or re-run with --force."
                exit 1
            fi
        fi
    fi

    infra_down
    rm -f "$DEV_LOCK_FILE"
    echo "✓ Infrastructure stopped"
}

case "${1:-}" in
    run|start) run ;;
    reset)     reset ;;
    status)    status ;;
    stop)      stop "$@" ;;
    *)         usage ;;
esac
