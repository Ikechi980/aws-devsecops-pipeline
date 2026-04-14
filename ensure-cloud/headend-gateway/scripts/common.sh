#!/usr/bin/env bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

source "$REPO_ROOT/scripts/common.sh"

# Source shared infrastructure configuration
if [ -f "$REPO_ROOT/infra/dev.env" ]; then
    set -a
    source "$REPO_ROOT/infra/dev.env"
    set +a
fi
