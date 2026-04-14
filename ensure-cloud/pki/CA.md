# Certificate Authority Overview

This document explains the Smallstep CA structure, what files do what, and what needs to be backed up for disaster recovery.

## How It Works

The CA uses a two-tier hierarchy:
1. **Root CA** - Self-signed certificate and private key. Signs the intermediate certificate. Kept offline in production (not used by step-ca directly).
2. **Intermediate CA** - Certificate and private key signed by the root. This is what actually signs client certificates.

The API authenticates to step-ca using JWT tokens:
1. The API signs a JWT token with the **provisioner private key** (`provisioner.key`)
2. The JWT includes the CSR hash and subject, signed with the provisioner's private key
3. Step-ca verifies the JWT signature using the **provisioner public key** (stored in `ca.json` under `authority.provisioners[].key`)
4. If valid, step-ca uses the **intermediate private key** to sign the certificate

This means:
- **provisioner.key** is only used by the API (ensure-pki-api container)
- **intermediate_ca_key** is only used by step-ca (step-ca container)
- They serve different purposes in the certificate issuance flow

## Certificate Issuance Flow

Here's what happens when a client requests a certificate:

1. **Client** → **API**: POST CSR to `/v1/certificates`
2. **API**: Validates CSR (CN, SANs) and authorizes by IP
3. **API**: Creates JWT token containing:
   - CSR hash (SHA-256 of the DER-encoded CSR)
   - Subject (the CN from the CSR)
   - Provisioner name ("ensure-pki")
   - Expiration (5 minutes)
4. **API**: Signs JWT with **provisioner.key** private key, includes `kid` in JWT header
5. **API** → **step-ca**: POST CSR + JWT token to step-ca's `/1.0/sign` endpoint
6. **step-ca**: Validates JWT:
   - Checks signature using provisioner public key from `ca.json`
   - Verifies `kid` matches a known provisioner
   - Checks expiration
   - Validates CSR hash matches
7. **step-ca**: Signs certificate using **intermediate_ca_key**
8. **step-ca** → **API**: Returns signed certificate
9. **API**: Builds full chain [issued cert, intermediate cert, root cert]
10. **API** → **Client**: Returns `{"chain": [...]}`

**Key Point:** The provisioner key is for *authentication* (proving the API is authorized to request certificates), while the intermediate key is for *signing* (actually creating the certificates).

## Directory Structure

```
infra/stepca/data/
├── config/
│   ├── ca.json           - Main CA configuration
│   └── defaults.json     - Client defaults (CA URL, fingerprint)
├── certs/
│   ├── root_ca.crt       - Root certificate (public)
│   └── intermediate_ca.crt - Intermediate certificate (public)
├── secrets/
│   ├── root_ca_key       - Root private key (encrypted)
│   ├── intermediate_ca_key - Intermediate private key (encrypted)
│   ├── password.txt      - Password for encrypted keys
│   └── provisioner.key   - Provisioner private key (for JWT signing)
└── db/                   - BadgerDB database (issued certificates, revocations)
```

## File Details

### Configuration Files

**ca.json** - Main CA configuration
- Points to certificate and key locations
- Defines DNS names the CA answers to
- Configures the provisioner (who can request certificates)
- Sets certificate duration limits (claims)
- Database settings

**defaults.json** - Client configuration
- CA URL
- Root certificate fingerprint
- Used by step-cli to connect to the CA

### Certificates (Public)

**root_ca.crt** - Root certificate
- Self-signed
- 100-year validity
- Used to verify the intermediate certificate
- Distributed to clients for trust validation

**intermediate_ca.crt** - Intermediate certificate
- Signed by the root CA
- 100-year validity
- This is what step-ca uses to sign client certificates
- Referenced in `ca.json` as `"crt"`

### Private Keys (Secret)

**root_ca_key** - Root private key
- Encrypted with password from `password.txt`
- Only used to sign the intermediate certificate (already done during init)
- Step-ca doesn't need this to run
- Keep this secure - it can sign new intermediate certificates

**intermediate_ca_key** - Intermediate private key
- Encrypted with password from `password.txt`
- Used by step-ca to sign all client certificates
- Referenced in `ca.json` as `"key"`
- Step-ca needs this to operate

**provisioner.key** - Provisioner private key (JWK)
- Decrypted version of the `encryptedKey` in ca.json
- Used by ensure-pki-api to sign JWT authentication tokens
- Contains both private and public key components, plus a `kid` (key ID)
- The matching public key is in `ca.json` under `authority.provisioners[].key`
- Step-ca validates JWT signatures using the public key from ca.json
- The API needs this file; step-ca doesn't directly use it
- The `kid` must be set in the `STEP_CA_PROVISIONER_KEY_ID` environment variable

**password.txt** - Encryption password
- Decrypts the root and intermediate private keys
- Step-ca needs this on startup
- Store securely - without it, you can't decrypt the keys

### Database

**db/** - BadgerDB database
- Tracks issued certificates
- Stores revocation information
- Can be regenerated (but you lose history)
- Not strictly necessary for disaster recovery if you're okay losing certificate history

## Critical Files for Disaster Recovery

To restore the CA after complete server loss, you need:

### Must Have (CA Cannot Function Without These)
1. `config/ca.json` - CA configuration
2. `certs/intermediate_ca.crt` - Intermediate certificate
3. `secrets/intermediate_ca_key` - Intermediate private key
4. `secrets/password.txt` - Key encryption password
5. `secrets/provisioner.key` - For the API to issue certificates

### Should Have (For Complete Restoration)
6. `certs/root_ca.crt` - Root certificate (for client trust)
7. `secrets/root_ca_key` - Root key (to issue new intermediates if needed)
8. `db/` - Certificate history and revocation data

### Can Regenerate
- `config/defaults.json` - Just points to the CA URL and fingerprint

## Backup Strategy

**Minimum viable backup:**
```bash
infra/stepca/data/config/ca.json
infra/stepca/data/certs/intermediate_ca.crt
infra/stepca/data/secrets/intermediate_ca_key
infra/stepca/data/secrets/password.txt
infra/stepca/data/secrets/provisioner.key
```

**Complete backup:**
```bash
infra/stepca/data/
```

**Important:** The password in `password.txt` should also be stored separately in a password manager or secure vault.

## What Happens If You Lose...

**Root private key** (`root_ca_key`)
- Can't create new intermediate certificates
- Current intermediate still works for its 100-year lifetime
- If intermediate is compromised, you need a new root CA (full redeployment to all clients)

**Intermediate private key** (`intermediate_ca_key`)
- CA can't sign any certificates
- Need to use root key to generate a new intermediate
- Clients already trust the root, so they'll accept the new intermediate

**Provisioner key** (`provisioner.key`)
- API can't request certificates
- Can regenerate from `encryptedKey` in ca.json using the password

**Password** (`password.txt`)
- Can't decrypt any private keys
- Can't start step-ca
- If you lose this, the CA is effectively dead

**Database** (`db/`)
- Lose history of what certificates were issued
- Lose revocation data
- CA still functions, just without history

**ca.json**
- Lose configuration
- Would need to recreate manually or from backups
- Without this, nothing works

## Security Notes

- **Never commit secrets to git** - `.gitignore` prevents this
- **Root key should be offline** - After CA init, you can remove `root_ca_key` from the server and store it separately
- **Password must be strong** - Production init script enforces 12+ characters
- **Provisioner key is powerful** - Anyone with this can request certificates for any `*.ensurelink.net` domain (subject to IP checks)
