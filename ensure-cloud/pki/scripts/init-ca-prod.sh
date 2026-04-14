#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CA_DIR="$PROJECT_ROOT/infra/stepca/data"

CA_NAME="EnsureLink Production CA"
CERT_TTL="876600h"  # 100 years
STEP_CA_ADDRESS=":9000"

DNS_NAMES=()

usage() {
    echo "Usage: $0 --dns <name> [--dns <name> ...]"
    echo ""
    echo "Initialize production Step CA configuration and certificates."
    echo ""
    echo "The first --dns value is used for --with-ca-url."
    exit 1
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dns)
                if [[ $# -lt 2 ]]; then
                    echo "ERROR: --dns requires a value"
                    usage
                fi
                if [[ -z "$2" ]]; then
                    echo "ERROR: --dns value must not be empty"
                    usage
                fi
                DNS_NAMES+=("$2")
                shift 2
                ;;
            *)
                echo "ERROR: unknown argument '$1'"
                usage
                ;;
        esac
    done

    if [[ ${#DNS_NAMES[@]} -eq 0 ]]; then
        echo "ERROR: at least one --dns value is required"
        usage
    fi
}

parse_args "$@"

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
    # Count CA material files
    ca_files=$(find "${CA_DIR}" -type f 2>/dev/null | wc -l)
    if [ "$ca_files" -gt 0 ]; then
        echo "ERROR: CA directory exists but is incomplete (missing: ${missing[*]})"
        echo "For production, manually remove the CA material first:"
        echo "  rm -rf ${CA_DIR}"
        exit 1
    fi
fi

mkdir -p "${CA_DIR}/config" "${CA_DIR}/certs" "${CA_DIR}/secrets"

echo "Creating production CA..."
echo ""
read -rsp "Enter a strong password for the CA (min 12 characters): " CA_PASSWORD
echo
read -rsp "Confirm password: " CA_PASSWORD_CONFIRM
echo

if [ "${CA_PASSWORD}" != "${CA_PASSWORD_CONFIRM}" ]; then
    echo "ERROR: Passwords do not match"
    exit 1
fi

if [ ${#CA_PASSWORD} -lt 12 ]; then
    echo "ERROR: Password must be at least 12 characters"
    exit 1
fi

echo "${CA_PASSWORD}" > "${CA_DIR}/secrets/password.txt"
chmod 600 "${CA_DIR}/secrets/password.txt"

echo ""
echo "Initializing ${CA_NAME}..."
printf 'Step CA DNS names: %s\n' "${DNS_NAMES[*]}"
echo "Step CA URL: https://${DNS_NAMES[0]}:9000"
echo ""

step_ca_init_args=(
    step
    ca
    init
    --deployment-type standalone
    --name "${CA_NAME}"
    --address "${STEP_CA_ADDRESS}"
    --provisioner "pki-api"
    --password-file /home/step/secrets/password.txt
    --provisioner-password-file /home/step/secrets/password.txt
    --with-ca-url "https://${DNS_NAMES[0]}:9000"
    --remote-management=false
)

for dns_name in "${DNS_NAMES[@]}"; do
    step_ca_init_args+=(--dns "${dns_name}")
done

docker run --rm \
    -v "${CA_DIR}":/home/step \
    -e STEPPATH=/home/step \
    --user "$(id -u):$(id -g)" \
    smallstep/step-cli:0.30.1 \
    "${step_ca_init_args[@]}"

# Replace with 100-year root certificate
echo "Creating 100-year root certificate..."
docker run --rm \
    -v "${CA_DIR}":/home/step \
    -e STEPPATH=/home/step \
    --user "$(id -u):$(id -g)" \
    smallstep/step-cli:0.30.1 \
    step certificate create \
        "${CA_NAME} Root CA" \
        /home/step/certs/root_ca.crt \
        /home/step/secrets/root_ca_key \
        --profile root-ca \
        --not-after "${CERT_TTL}" \
        --password-file /home/step/secrets/password.txt \
        --no-password --insecure --force

# Replace with 100-year intermediate certificate
echo "Creating 100-year intermediate certificate..."
docker run --rm \
    -v "${CA_DIR}":/home/step \
    -e STEPPATH=/home/step \
    --user "$(id -u):$(id -g)" \
    smallstep/step-cli:0.30.1 \
    step certificate create \
        "${CA_NAME} Intermediate CA" \
        /home/step/certs/intermediate_ca.crt \
        /home/step/secrets/intermediate_ca_key \
        --profile intermediate-ca \
        --ca /home/step/certs/root_ca.crt \
        --ca-key /home/step/secrets/root_ca_key \
        --not-after "${CERT_TTL}" \
        --password-file /home/step/secrets/password.txt \
        --no-password --insecure --force

generate_provisioner_key

# Configure certificate TTL
echo "Configuring certificate TTL..."
jq --arg ttl "${CERT_TTL}" '.authority.claims = {
    "minTLSCertDuration": "5m",
    "maxTLSCertDuration": $ttl,
    "defaultTLSCertDuration": $ttl,
    "disableRenewal": false
}' "${CA_DIR}/config/ca.json" > "${CA_DIR}/config/ca.json.tmp"
mv "${CA_DIR}/config/ca.json.tmp" "${CA_DIR}/config/ca.json"

echo ""
echo "✓ CA initialized successfully!"
echo ""
echo "Provisioner key: ${CA_DIR}/secrets/provisioner.key"
echo "Root cert:       ${CA_DIR}/certs/root_ca.crt"

if command -v jq >/dev/null 2>&1; then
    KID=$(jq -r '.authority.provisioners[0].key.kid // empty' "${CA_DIR}/config/ca.json" || true)
    if [ -n "${KID}" ]; then
        echo "Provisioner kid: ${KID}"
        echo ""
        echo "Add this to your deployment configuration:"
        echo "  STEP_CA_PROVISIONER_KEY_ID=${KID}"
        echo ""
        echo "Store the CA password securely and provide the CA material to step-ca at runtime."
    fi
fi
