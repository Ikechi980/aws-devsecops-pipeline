#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

SERVICES=(
    pki
    headend-gateway
    headend-api
    core-change-publisher
)

run_service_tests() {
    local service="$1"
    local script_path="$REPO_ROOT/$service/scripts/dev.sh"
    local status

    print_header "${service} integration tests"

    "$script_path" test --reset
    status=$?

    if [[ "$status" -eq 0 ]]; then
        echo "PASS: ${service} integration tests"
        return 0
    fi

    echo "FAIL: ${service} integration tests (exit ${status})"
    return "$status"
}

main() {
    local failures=()
    local service

    for service in "${SERVICES[@]}"; do
        if ! run_service_tests "$service"; then
            failures+=("$service")
        fi
    done

    print_header "Integration Test Summary"

    if [[ ${#failures[@]} -eq 0 ]]; then
        echo "All integration test flows passed."
        return 0
    fi

    printf 'Failed integration test flows: %s\n' "${failures[*]}"
    return 1
}

main "$@"
