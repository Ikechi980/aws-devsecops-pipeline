#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOOLCHAIN_TOML="$REPO_ROOT/rust-toolchain.toml"
TOOLCHAIN_FILE="$REPO_ROOT/rust-toolchain"

if [[ -f "$TOOLCHAIN_TOML" ]]; then
    version="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$TOOLCHAIN_TOML" | head -n1)"
    if [[ -n "${version:-}" ]]; then
        printf '%s\n' "$version"
        exit 0
    fi
fi

if [[ -f "$TOOLCHAIN_FILE" ]]; then
    version="$(grep -Ev '^[[:space:]]*($|#)' "$TOOLCHAIN_FILE" | head -n1 || true)"
    if [[ -n "${version:-}" ]]; then
        printf '%s\n' "$version"
        exit 0
    fi
fi

echo "Error: unable to determine Rust toolchain version from rust-toolchain.toml or rust-toolchain" >&2
exit 1
