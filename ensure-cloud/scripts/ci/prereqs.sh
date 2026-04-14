#!/usr/bin/env bash
set -uo pipefail

CI_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$CI_SCRIPT_DIR/../common.sh"
RUST_VERSION="$("$CI_SCRIPT_DIR/rust-version.sh")"
CARGO_LAMBDA_VERSION="$("$CI_SCRIPT_DIR/cargo-lambda-version.sh")"

usage() {
    cat <<'EOF'
Usage: ./scripts/ci/prereqs.sh [security|lint|test]

  security  Check self-hosted runner prerequisites for security scanning
  lint      Check self-hosted runner prerequisites for Rust linting
  test      Check self-hosted runner prerequisites for integration tests
EOF
}

missing=()
version_mismatch=()

require_command() {
    local command_name="$1"
    local label="${2:-$1}"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        missing+=("$label")
    fi
}

require_check() {
    local label="$1"
    shift

    if ! "$@" >/dev/null 2>&1; then
        missing+=("$label")
    fi
}

require_version() {
    local command_name="$1"
    local expected_version="$2"
    local actual

    if ! command -v "$command_name" >/dev/null 2>&1; then
        return 0
    fi

    actual="$("$command_name" --version 2>/dev/null || true)"
    if [[ "$actual" != *" ${expected_version} "* ]]; then
        version_mismatch+=("${command_name}: expected ${expected_version}, found ${actual:-unknown}")
    fi
}

require_output_version() {
    local label="$1"
    local expected_version="$2"
    shift 2

    local actual
    actual="$("$@" 2>/dev/null || true)"

    if [[ "$actual" != *" ${expected_version} "* ]]; then
        version_mismatch+=("${label}: expected ${expected_version}, found ${actual:-unknown}")
    fi
}

print_result() {
    local target="$1"

    if [[ ${#missing[@]} -eq 0 && ${#version_mismatch[@]} -eq 0 ]]; then
        echo "All ${target} prerequisites are available on this self-hosted runner."
        return 0
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "Missing ${target} prerequisites on this self-hosted runner:"
        printf '  - %s\n' "${missing[@]}"
        echo ""
    fi

    if [[ ${#version_mismatch[@]} -gt 0 ]]; then
        echo "${target^} prerequisite version mismatches on this self-hosted runner:"
        printf '  - %s\n' "${version_mismatch[@]}"
        echo ""
    fi

    echo "Install or align the required tools on the runner host and ensure they are available in PATH before rerunning the workflow."
    return 1
}

check_security() {
    require_command docker
    require_check "docker compose plugin" docker compose version
    require_check "docker daemon access" docker info
    print_result "security"
}

check_lint() {
    require_command cargo
    require_command rustc
    require_check "cargo fmt (rustfmt component)" cargo fmt --version
    require_check "cargo clippy (clippy component)" cargo clippy --version
    require_version cargo "$RUST_VERSION"
    require_version rustc "$RUST_VERSION"
    print_result "lint"
}

check_test() {
    require_command cargo
    require_command rustc
    require_command cargo-lambda
    require_command aws
    require_command jq
    require_command curl
    require_command openssl
    require_command docker
    require_check "docker compose plugin" docker compose version
    require_check "docker daemon access" docker info
    require_check "cargo lambda subcommand" cargo lambda --version
    require_output_version cargo-lambda "$CARGO_LAMBDA_VERSION" cargo lambda --version
    require_version cargo "$RUST_VERSION"
    require_version rustc "$RUST_VERSION"
    print_result "integration test"
}

case "${1:-}" in
    security)
        check_security
        ;;
    lint)
        check_lint
        ;;
    test)
        check_test
        ;;
    *)
        usage
        exit 1
        ;;
esac
