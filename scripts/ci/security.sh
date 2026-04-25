#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

TRIVY_IMAGE="ghcr.io/aquasecurity/trivy:0.68.2"
GITLEAKS_IMAGE="ghcr.io/gitleaks/gitleaks:v8.24.2"
TRIVY_CACHE_HOST_DIR="${TMPDIR:-/tmp}/infra-trivy-cache"
IAC_TARGET="infra/iac"

require_command() {
    local command_name="$1"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Error: required command '${command_name}' is not available in PATH"
        exit 1
    fi
}

run_step() {
    local name="$1"
    local status
    shift

    print_header "$name"

    "$@"
    status=$?

    if [[ "$status" -eq 0 ]]; then
        echo "PASS: ${name}"
        return 0
    fi

    echo "FAIL: ${name} (exit ${status})"
    return "$status"
}

run_trivy_iac() {
    mkdir -p "$TRIVY_CACHE_HOST_DIR"

    docker run --rm \
        --user "$(id -u):$(id -g)" \
        -e HOME=/tmp \
        -e TRIVY_CACHE_DIR=/trivy-cache \
        -v "$REPO_ROOT:/work" \
        -v "$TRIVY_CACHE_HOST_DIR:/trivy-cache" \
        --workdir /work \
        --entrypoint trivy \
        "$TRIVY_IMAGE" \
        config \
        --severity HIGH,CRITICAL \
        --exit-code 1 \
        "$IAC_TARGET"
}

main() {
    local failures=()

    require_command docker
    check_docker

    if ! run_step "Trivy IaC" run_trivy_iac; then
        failures+=("trivy-iac")
    fi

    if ! run_step "Gitleaks" \
        docker run --rm \
        --user "$(id -u):$(id -g)" \
        -e HOME=/tmp \
        -v "$REPO_ROOT:/repo" \
        --workdir /repo \
        --entrypoint gitleaks \
        "$GITLEAKS_IMAGE" \
        detect \
        --source . \
        --verbose \
        --redact; then
        failures+=("gitleaks")
    fi

    print_header "Security Summary"

    if [[ ${#failures[@]} -eq 0 ]]; then
        echo "All security checks passed."
        return 0
    fi

    printf 'Failed security checks: %s\n' "${failures[*]}"
    return 1
}

main "$@"
