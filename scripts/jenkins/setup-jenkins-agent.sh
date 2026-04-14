#!/usr/bin/env bash
# =============================================================================
# setup-jenkins-agent.sh
#
# Run once (as ec2-user with sudo) on the Jenkins EC2 instance to install
# all build tools the pipeline needs. Safe to re-run — every install step
# is idempotent.
#
# Usage:
#   chmod +x scripts/jenkins/setup-jenkins-agent.sh
#   sudo ./scripts/jenkins/setup-jenkins-agent.sh
# =============================================================================
set -euo pipefail

RUST_VERSION="1.93.0"
CARGO_LAMBDA_VERSION="1.9.1"
ZIG_VERSION="0.13.0"

# Persistent cache dirs owned by the jenkins user.
# These survive across builds and are referenced in the Jenkinsfile env block.
JENKINS_CACHE_ROOT="/var/cache/jenkins"
RUSTUP_HOME="${JENKINS_CACHE_ROOT}/rustup"
CARGO_HOME="${JENKINS_CACHE_ROOT}/cargo"
CARGO_TARGET_DIR="${JENKINS_CACHE_ROOT}/cargo-target"

log() { echo ">>> $*"; }

# --------------------------------------------------------------------------
# 1. System packages
# --------------------------------------------------------------------------
log "Updating system packages..."
dnf update -y

log "Installing system dependencies..."
# Note: curl-minimal is pre-installed on AL2023 and conflicts with full curl — skip it
dnf install -y \
  git \
  wget \
  jq \
  unzip \
  tar \
  gzip \
  openssl \
  openssl-devel \
  pkgconfig \
  gcc \
  make \
  docker \
  python3-pip

# --------------------------------------------------------------------------
# 2. Docker daemon + compose plugin
# --------------------------------------------------------------------------
log "Configuring Docker..."
systemctl enable docker
systemctl start docker

# Add jenkins user to docker group so the pipeline can run docker commands
usermod -aG docker jenkins

# Install Docker Compose plugin (v2 — 'docker compose' not 'docker-compose')
DOCKER_CLI_PLUGINS_DIR="/usr/local/lib/docker/cli-plugins"
mkdir -p "${DOCKER_CLI_PLUGINS_DIR}"
COMPOSE_VERSION="v2.27.1"
# Detect architecture — EC2 Graviton instances are aarch64
ARCH="$(uname -m)"
COMPOSE_URL="https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-linux-${ARCH}"
if ! docker compose version &>/dev/null; then
  log "Installing Docker Compose plugin ${COMPOSE_VERSION} (${ARCH})..."
  curl -fsSL "${COMPOSE_URL}" -o "${DOCKER_CLI_PLUGINS_DIR}/docker-compose"
  chmod +x "${DOCKER_CLI_PLUGINS_DIR}/docker-compose"
fi
docker compose version

# --------------------------------------------------------------------------
# 3. Persistent cache directories for Rust (owned by jenkins)
# --------------------------------------------------------------------------
log "Creating persistent cache directories..."
mkdir -p "${RUSTUP_HOME}" "${CARGO_HOME}" "${CARGO_TARGET_DIR}"
chown -R jenkins:jenkins "${JENKINS_CACHE_ROOT}"

# --------------------------------------------------------------------------
# 4. Rust toolchain (installed as jenkins user into persistent dirs)
# --------------------------------------------------------------------------
log "Installing Rust ${RUST_VERSION} as jenkins user..."
sudo -u jenkins env \
  RUSTUP_HOME="${RUSTUP_HOME}" \
  CARGO_HOME="${CARGO_HOME}" \
  bash -c '
    if ! command -v rustup &>/dev/null; then
      curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain none --profile minimal
    fi

    export PATH="${CARGO_HOME}/bin:${PATH}"

    rustup toolchain install "'"${RUST_VERSION}"'" \
      --profile minimal \
      --component rustfmt \
      --component clippy

    rustup default "'"${RUST_VERSION}"'"
    rustc --version
    cargo --version
  '

# --------------------------------------------------------------------------
# 5. cargo-lambda (installed as jenkins user)
# --------------------------------------------------------------------------
log "Installing cargo-lambda ${CARGO_LAMBDA_VERSION}..."
sudo -u jenkins env \
  RUSTUP_HOME="${RUSTUP_HOME}" \
  CARGO_HOME="${CARGO_HOME}" \
  PATH="${CARGO_HOME}/bin:/usr/local/bin:${PATH}" \
  bash -c '
    if ! cargo lambda --version 2>/dev/null | grep -q " '"${CARGO_LAMBDA_VERSION}"' "; then
      cargo install cargo-lambda --locked --version "'"${CARGO_LAMBDA_VERSION}"'"
    fi
    cargo lambda --version
  '

# --------------------------------------------------------------------------
# 6. Zig (cross-compilation linker used by cargo-lambda)
# --------------------------------------------------------------------------
log "Installing Zig ${ZIG_VERSION}..."
ZIG_DIR="/opt/zig"
# Map uname -m to the Zig archive naming convention
case "$(uname -m)" in
  x86_64)  ZIG_ARCH="x86_64"  ;;
  aarch64) ZIG_ARCH="aarch64" ;;
  *)       echo "ERROR: unsupported architecture $(uname -m)"; exit 1 ;;
esac
ZIG_TARBALL="zig-linux-${ZIG_ARCH}-${ZIG_VERSION}.tar.xz"
ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/${ZIG_TARBALL}"

if [ ! -x "${ZIG_DIR}/zig" ]; then
  log "Downloading Zig for ${ZIG_ARCH}..."
  curl -fsSL "${ZIG_URL}" -o "/tmp/${ZIG_TARBALL}"
  mkdir -p "${ZIG_DIR}"
  tar -xf "/tmp/${ZIG_TARBALL}" -C "${ZIG_DIR}" --strip-components=1
  rm -f "/tmp/${ZIG_TARBALL}"
fi
ln -sf "${ZIG_DIR}/zig" /usr/local/bin/zig
zig version

# --------------------------------------------------------------------------
# 7. Trivy (vulnerability scanner)
# --------------------------------------------------------------------------
log "Installing Trivy..."
if ! command -v trivy &>/dev/null; then
  curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh \
    | sh -s -- -b /usr/local/bin
fi
trivy --version

# --------------------------------------------------------------------------
# 8. AWS CLI v2
# --------------------------------------------------------------------------
log "Checking AWS CLI..."
if ! command -v aws &>/dev/null; then
  log "Installing AWS CLI v2..."
  case "$(uname -m)" in
    x86_64)  AWS_ARCH="x86_64"  ;;
    aarch64) AWS_ARCH="aarch64" ;;
  esac
  curl "https://awscli.amazonaws.com/awscli-exe-linux-${AWS_ARCH}.zip" -o /tmp/awscliv2.zip
  unzip -q /tmp/awscliv2.zip -d /tmp/
  /tmp/aws/install
  rm -rf /tmp/aws /tmp/awscliv2.zip
fi
aws --version

# --------------------------------------------------------------------------
# 9. Write /etc/profile.d snippet so interactive shells pick up the paths
# --------------------------------------------------------------------------
log "Writing PATH profile snippet..."
cat > /etc/profile.d/jenkins-build-tools.sh <<EOF
# Added by setup-jenkins-agent.sh
export RUSTUP_HOME="${RUSTUP_HOME}"
export CARGO_HOME="${CARGO_HOME}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR}"
export PATH="${CARGO_HOME}/bin:/opt/zig:/usr/local/bin:\${PATH}"
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL="sparse"
export CARGO_BUILD_JOBS="4"
EOF

# --------------------------------------------------------------------------
# Done
# --------------------------------------------------------------------------
log "=== Agent setup complete ==="
log "Rust:         $(sudo -u jenkins env RUSTUP_HOME=${RUSTUP_HOME} CARGO_HOME=${CARGO_HOME} PATH=${CARGO_HOME}/bin:$PATH rustc --version)"
log "Cargo:        $(sudo -u jenkins env RUSTUP_HOME=${RUSTUP_HOME} CARGO_HOME=${CARGO_HOME} PATH=${CARGO_HOME}/bin:$PATH cargo --version)"
log "cargo-lambda: $(sudo -u jenkins env RUSTUP_HOME=${RUSTUP_HOME} CARGO_HOME=${CARGO_HOME} PATH=${CARGO_HOME}/bin:$PATH cargo lambda --version)"
log "Zig:          $(zig version)"
log "Trivy:        $(trivy --version | head -1)"
log "Docker:       $(docker --version)"
log "AWS CLI:      $(aws --version)"
log ""
log "IMPORTANT: Restart Jenkins so it picks up the new docker group membership:"
log "  sudo systemctl restart jenkins"
