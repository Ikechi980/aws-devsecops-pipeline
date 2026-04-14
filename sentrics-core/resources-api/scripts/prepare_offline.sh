#!/usr/bin/env bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/.."
source "${SCRIPT_DIR}/common.sh"

cleanup() {
    if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
        infra_down || true
    fi
    release_lock || true
}

trap cleanup EXIT INT TERM

echo "Preparing SQLx offline mode..."

acquire_lock "resources-api prepare_offline"
require_ports_free 4566 5432
infra_up
wait_for_localstack_init
(cd "$PROJECT_ROOT" && MIGRATE_PORT=9010 ./scripts/migrate.sh)

cd "$PROJECT_ROOT"
cargo sqlx prepare
echo "SQLx offline data updated in .sqlx/"
