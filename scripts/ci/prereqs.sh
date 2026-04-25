#!/usr/bin/env bash
set -uo pipefail

CI_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$CI_SCRIPT_DIR/../common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/ci/prereqs.sh [security]

  security  Check self-hosted runner prerequisites for infrastructure security scanning
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

case "${1:-}" in
    security)
        check_security
        ;;
    *)
        usage
        exit 1
        ;;
esac
