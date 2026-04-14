#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/check.sh [all|security|lint|test]

  all       Run all pre-merge gates
  security  Run source security scanners
  lint      Run cargo fmt --check and cargo clippy --all-targets
  test      Run service integration test flows
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
    local gate

    for gate in security lint test; do
        if ! run_gate "$gate" "$SCRIPT_DIR/ci/${gate}.sh"; then
            failures+=("$gate")
        fi
    done

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
    security|lint|test)
        "$SCRIPT_DIR/ci/${1}.sh"
        ;;
    *)
        usage
        exit 1
        ;;
esac
