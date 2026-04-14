# Ensure PKI

Certificate authority API service for issuing TLS certificates to Ensure headends and services.

## What It Does

This service sits in front of a Smallstep CA and handles authorization. It accepts CSRs for `*.ensurelink.net`, validates them, checks the requester's IP against the community's network IP, and forwards valid requests to step-ca for signing.

## API

**POST /v1/certificates** - Issue a certificate
- Request: `{"csr": "<PEM>"}`
- CN must be `<community_id>.ensurelink.net`
- SANs must end with `.ensurelink.net`
- Authorizes by IP (CIDR bypass or community lookup)
- Returns: `201` with `{"chain": [<issued cert>, <intermediate cert>, <root cert>]}`

**GET /v1/health** - Health check

## Configuration

All configuration is via environment variables. See `.env.example` for available options.

## Development

Shared development infrastructure is managed at the repository root and includes Step CA and mock-systems-api.

Run pki with shared infrastructure:
```bash
cd pki
./scripts/dev.sh run
```

The script creates a `.env` file from `.env.example` on first run.

Services run on:
- API: `localhost:8080`
- step-ca: `localhost:9100`
- mock-systems-api: `localhost:8081`

### Running Tests

All tests are end-to-end tests that require the full development environment:

```bash
./scripts/dev.sh test
```

The tests exercise the full application stack including:
- Certificate issuance through step-ca
- Community lookup via mock-systems-api
- Authorization and validation logic
- HTTP layer behavior

### Testing with curl

Generate a CSR and request a certificate:

```bash
openssl req -new -newkey rsa:2048 -nodes \
  -keyout /tmp/test.key \
  -subj "/CN=alpha.ensurelink.net" \
  -out /tmp/test.csr

curl -X POST http://localhost:8080/v1/certificates \
  -d "{\"csr\": \"$(cat /tmp/test.csr | sed 's/$/\\n/' | tr -d '\n')\"}" \
  | jq .
```

## Deployment Notes

The PKI stack consists of two deployables: the pki-api service and a step-ca instance.

**Runtime and artifacts**
- Build the pki-api image from `infra/pki-api/Dockerfile`.
- Build the step-ca image from `infra/stepca/Dockerfile`.

**AWS resources and wiring**
- pki-api exposes `/v1/certificates` and `/v1/health`.
- step-ca must be reachable from pki-api at `STEP_CA_URL` (same task, same cluster, or separate).

**Secrets and CA material**
- Generate the step-ca material before deployment with `./scripts/init-ca-prod.sh --dns <name> [--dns <name> ...]`.
- The first `--dns` value becomes the CA URL written to `defaults.json`; every `--dns` value is added as a SAN on the served step-ca certificate.
- Example:
```bash
./scripts/init-ca-prod.sh \
  --dns dev-stepca.ensure-cloud-dev.internal \
  --dns step-ca
```
- step-ca requires the root and intermediate certificates and the provisioner key at runtime.
- pki-api must be able to read `STEP_CA_CERTS_DIR` and `STEP_CA_PROVISIONER_KEY_PATH`.
- Store these secrets securely and provide them via your preferred secret or volume mechanism.

**Networking**
- pki-api must be reachable by headends over the Ensure WireGuard network.
- pki-api needs outbound HTTPS access to step-ca and to the EMS API.

**Runtime configuration**
- pki-api: `SYSTEM_API_BASE`, `STEP_CA_URL`, `STEP_CA_CERTS_DIR`, `STEP_CA_PROVISIONER_NAME`,
  `STEP_CA_PROVISIONER_KEY_ID`, `STEP_CA_PROVISIONER_KEY_PATH`, `STEP_CA_TOKEN_TTL_SECS`, `HOST`, `PORT`,
  `ALLOWED_CIDRS`.
- pki-api (optional): `RUST_LOG`.
- step-ca: configured by its own image/runtime for certificates, provisioner, and storage.

**Constraints**
- The API only accepts CSRs for `*.ensurelink.net`, and the CN must be `<community_id>.ensurelink.net`.
- Requests are authorized by WireGuard client IP and community network configuration found in EMS.
