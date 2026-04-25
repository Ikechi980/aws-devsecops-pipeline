#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/check.sh [all|security]

  all       Run all pre-merge gates
  security  Run infrastructure security scanners
EOF
}

run_gate() {
    local name="$1"
    local script_path="$2"
    local status

    print_header "Running ${name}"

    "$script_path"
    status=$?

    if [[ "$status" -eq 0 ]]; then
        echo "PASS: ${name}"
        return 0
    fi

    echo "FAIL: ${name} (exit ${status})"
    return "$status"
}

run_all() {
    local failures=()

    if ! run_gate "security" "$SCRIPT_DIR/ci/security.sh"; then
        failures+=("security")
    fi

    print_header "Merge Gate Summary"

    if [[ ${#failures[@]} -eq 0 ]]; then
        echo "All merge gates passed."
        return 0
    fi

    printf 'Failed gates: %s\n' "${failures[*]}"
    return 1
}

case "${1:-}" in
    all)
        run_all
        ;;
    security)
        "$SCRIPT_DIR/ci/security.sh"
        ;;
    *)
        usage
        exit 1
        ;;
esac
