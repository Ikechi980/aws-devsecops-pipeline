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

 

check_resources_api() {
    if ! curl -s "$RESOURCES_API_HEALTH_URL_LOCAL" > /dev/null 2>&1; then
        echo "⚠️  resources-api is not running."
        echo ""
        echo "yardi-sync requires resources-api to be running for full integration."
        echo "Start it in another terminal:"
        echo "  cd $REPO_ROOT/resources-api && cargo lambda watch -p resources-api --bin resources-api --invoke-address $LOOPBACK_HOST"
        echo ""
        echo "Alternatively, you can point to a staging environment by setting:"
        echo "  RESOURCES_API_BASE_URL=https://staging.example.com/v1"
        echo ""
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 0
        fi
        echo ""
    fi
}
