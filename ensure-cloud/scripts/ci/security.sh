#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

SEMGRP_IMAGE="semgrep/semgrep:1.155.0"
TRIVY_IMAGE="ghcr.io/aquasecurity/trivy:0.68.2"
GITLEAKS_IMAGE="ghcr.io/gitleaks/gitleaks:v8.24.2"
HADOLINT_IMAGE="hadolint/hadolint:v2.12.0-debian"
TRIVY_SCANNERS="vuln,misconfig"
TRIVY_SEVERITIES="HIGH,CRITICAL"
SEMGRP_TARGETS=(
    pki
    headend-gateway
    headend-api
    core-change-publisher
    buildspec-security-scan.yaml
)
TRIVY_TARGETS=(
    pki
    headend-gateway
    headend-api
    core-change-publisher
)
HADOLINT_TARGETS=(
    headend-gateway/infra/headend-gateway/Dockerfile
    pki/infra/pki-api/Dockerfile
    pki/infra/stepca/Dockerfile
)

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

run_hadolint() {
    local relative_file
    local status=0

    for relative_file in "${HADOLINT_TARGETS[@]}"; do
        echo "Linting ${relative_file}"
        if ! docker run --rm \
            --user "$(id -u):$(id -g)" \
            -e HOME=/tmp \
            -v "$REPO_ROOT:/work" \
            --workdir /work \
            --entrypoint hadolint \
            "$HADOLINT_IMAGE" \
            --failure-threshold error \
            "$relative_file"; then
            status=1
        fi
    done

    return "$status"
}

run_trivy() {
    local relative_target
    local status=0

    for relative_target in "${TRIVY_TARGETS[@]}"; do
        echo "Scanning ${relative_target}"
        if ! docker run --rm \
            --user "$(id -u):$(id -g)" \
            -e HOME=/tmp \
            -e TRIVY_CACHE_DIR=/tmp/trivy-cache \
            -v "$REPO_ROOT:/work" \
            --workdir /work \
            --entrypoint trivy \
            "$TRIVY_IMAGE" \
            fs \
            --scanners "$TRIVY_SCANNERS" \
            --severity "$TRIVY_SEVERITIES" \
            --no-progress \
            --exit-code 1 \
            "$relative_target"; then
            status=1
        fi
    done

    return "$status"
}

main() {
    local failures=()

    require_command docker
    check_docker

    if ! run_step "Semgrep" \
        docker run --rm \
        --user "$(id -u):$(id -g)" \
        -e HOME=/tmp \
        -v "$REPO_ROOT:/src" \
        --workdir /src \
        --entrypoint semgrep \
        "$SEMGRP_IMAGE" \
        scan \
        --error \
        --config p/security-audit \
        --config p/owasp-top-ten \
        --config p/secrets \
        --config p/docker \
        --config p/javascript \
        --config p/rust \
        --metrics=off \
        "${SEMGRP_TARGETS[@]}"; then
        failures+=("semgrep")
    fi

    if ! run_step "Trivy" run_trivy; then
        failures+=("trivy")
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

    if ! run_step "Hadolint" run_hadolint; then
        failures+=("hadolint")
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
