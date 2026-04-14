#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

CRATES=(
    resources-api
    resources-change-logger
    yardi-sync
)

require_command() {
    local command_name="$1"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Error: required command '${command_name}' is not available in PATH"
        exit 1
    fi
}

run_cargo_check() {
    local crate="$1"
    local label="$2"
    local status
    shift 2

    print_header "${crate} ${label}"

    (cd "$REPO_ROOT/$crate" && "$@")
    status=$?

    if [[ "$status" -eq 0 ]]; then
        echo "PASS: ${crate} ${label}"
        return 0
    fi

    echo "FAIL: ${crate} ${label} (exit ${status})"
    return "$status"
}

main() {
    local failures=()
    local crate

    require_command cargo

    for crate in "${CRATES[@]}"; do
        if ! run_cargo_check "$crate" "cargo fmt --check" cargo fmt --check; then
            failures+=("${crate}:fmt")
        fi

        if ! run_cargo_check "$crate" "cargo clippy --all-targets" env SQLX_OFFLINE=true cargo clippy --all-targets; then
            failures+=("${crate}:clippy")
        fi
    done

    print_header "Lint Summary"

    if [[ ${#failures[@]} -eq 0 ]]; then
        echo "All lint checks passed."
        return 0
    fi

    printf 'Failed lint checks: %s\n' "${failures[*]}"
    return 1
}

main "$@"
