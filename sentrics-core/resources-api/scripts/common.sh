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

# Fixed local development database configuration.
export DB_USER="postgres"
export DB_PASSWORD="password"
export DB_NAME="resources_db"
export DB_PORT="5432"
export DB_HOST="localhost"

export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"

 
