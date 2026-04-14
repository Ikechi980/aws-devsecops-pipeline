#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CA_DIR="$REPO_ROOT/infra/stepca/data"
CA_NAME="EnsureLink Local CA"
CERT_TTL="8760h"  # 1 year

generate_provisioner_key() {
    local jwe
    jwe=$(jq -r '.authority.provisioners[0].encryptedKey // empty' "${CA_DIR}/config/ca.json")
    if [ -z "${jwe}" ]; then
        echo "Unable to locate provisioner encryptedKey in ${CA_DIR}/config/ca.json"
        return 1
    fi

    echo "Exporting provisioner key..."
    docker run --rm \
        -e STEPPATH=/home/step \
        -e JWE="${jwe}" \
        -v "${CA_DIR}":/home/step \
        smallstep/step-cli:0.30.1 \
        sh -c 'echo "$JWE" | step crypto jwe decrypt --password-file /home/step/secrets/password.txt' \
        > "${CA_DIR}/secrets/provisioner.key"
    chmod 600 "${CA_DIR}/secrets/provisioner.key"
}

REQUIRED_FILES=(
    "${CA_DIR}/config/ca.json"
    "${CA_DIR}/certs/root_ca.crt"
    "${CA_DIR}/secrets/provisioner.key"
)

missing=()
for file in "${REQUIRED_FILES[@]}"; do
    if [ ! -f "$file" ]; then
        missing+=("$file")
    fi
done

# If only the provisioner key is missing, we can regenerate it
if [ ${#missing[@]} -eq 1 ] && [ "${missing[0]}" = "${CA_DIR}/secrets/provisioner.key" ] && [ -f "${CA_DIR}/config/ca.json" ]; then
    generate_provisioner_key
    echo "✓ Provisioner key regenerated"
    exit 0
fi

# If nothing is missing, we're done
if [ ${#missing[@]} -eq 0 ]; then
    echo "✓ CA already initialized at ${CA_DIR}"
    exit 0
fi

# Check for partial initialization
if [ -d "${CA_DIR}" ]; then
    ca_files=$(find "${CA_DIR}" -type f 2>/dev/null | wc -l)
    if [ "$ca_files" -gt 0 ]; then
        echo "Existing CA material incomplete. Removing and reinitializing..."
        rm -rf "${CA_DIR}"
    fi
fi

mkdir -p "${CA_DIR}/config" "${CA_DIR}/certs" "${CA_DIR}/secrets"

echo "changeit" > "${CA_DIR}/secrets/password.txt"
chmod 600 "${CA_DIR}/secrets/password.txt"

echo ""
echo "Initializing ${CA_NAME}..."
echo ""

docker run --rm \
    -v "${CA_DIR}":/home/step \
    -e STEPPATH=/home/step \
    --user "$(id -u):$(id -g)" \
    smallstep/step-cli:0.30.1 \
    step ca init \
        --deployment-type standalone \
        --name "${CA_NAME}" \
        --dns "step-ca" \
        --dns "localhost" \
        --address ":9000" \
        --provisioner "pki-api" \
        --password-file /home/step/secrets/password.txt \
        --provisioner-password-file /home/step/secrets/password.txt \
        --with-ca-url https://step-ca:9000 \
        --remote-management=false

generate_provisioner_key

# Configure certificate TTL
jq --arg ttl "${CERT_TTL}" '.authority.claims = {
    "minTLSCertDuration": "5m",
    "maxTLSCertDuration": $ttl,
    "defaultTLSCertDuration": $ttl,
    "disableRenewal": false
}' "${CA_DIR}/config/ca.json" > "${CA_DIR}/config/ca.json.tmp"

mv "${CA_DIR}/config/ca.json.tmp" "${CA_DIR}/config/ca.json"

# Make sure root CA cert is world-readable for nginx
chmod 644 "${CA_DIR}/certs/root_ca.crt"

echo "✓ CA initialized at ${CA_DIR}"
