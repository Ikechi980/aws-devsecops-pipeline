// =============================================================================
// Sentrics Event-Driven Platform — Jenkins Pipeline  (speed-test mode)
//
//   1. Verify toolchain versions (fail fast on version drift)
//   2. Parallel Lambda builds  : ensure-cloud Lambdas | sentrics-core Lambdas
//   3. Parallel Docker builds  : headend-gateway | pki-api | stepca | yardi-sync
//      └─ exports each image as a .tar.gz for archiving
//   4. Trivy scan — counts CVEs per artifact, writes reports + summary CSVs
//      ├─ Lambda zip artifacts  → trivy-lambda-scan.txt  + lambda-cve-summary.csv
//      └─ Docker images         → trivy-docker-scan.txt  + docker-cve-summary.csv
//   5. Archive all artifacts in Jenkins
//   6. Cache stats report
//
// Caching strategy
//   RUSTUP_HOME, CARGO_HOME, and CARGO_TARGET_DIR all point to persistent
//   directories on the Jenkins agent filesystem (/var/cache/jenkins/*).
//   They survive across builds on the same agent, giving warm incremental
//   compile times.  No S3 cache is used.
// =============================================================================

pipeline {
    agent any

    options {
        buildDiscarder(logRotator(numToKeepStr: '30'))
        timeout(time: 30, unit: 'MINUTES')
        disableConcurrentBuilds()
    }

    // -------------------------------------------------------------------------
    // Build parameters
    // -------------------------------------------------------------------------
    parameters {
        string(
            name: 'RELEASE_SHA',
            defaultValue: '',
            description: 'Override release SHA (defaults to GIT_COMMIT)'
        )
        booleanParam(
            name: 'SKIP_DOCKER_BUILDS',
            defaultValue: false,
            description: 'Skip Docker image builds (useful for Lambda-only changes)'
        )
        string(
            name: 'TRIVY_SEVERITY',
            defaultValue: 'HIGH,CRITICAL',
            description: 'Severity levels to include in the scan'
        )
        string(
            name: 'MAX_CRITICAL',
            defaultValue: '0',
            description: 'Max CRITICAL CVEs allowed across all artifacts — publish is blocked if total exceeds this'
        )
        string(
            name: 'MAX_HIGH',
            defaultValue: '0',
            description: 'Max HIGH CVEs allowed across all artifacts — publish is blocked if total exceeds this'
        )
        string(
            name: 'STEPCA_MAX_CRITICAL',
            defaultValue: '2',
            description: 'Max CRITICAL CVEs allowed for the stepca image'
        )
        string(
            name: 'STEPCA_MAX_HIGH',
            defaultValue: '0',
            description: 'Max HIGH CVEs allowed for the stepca image'
        )
        booleanParam(
            name: 'ENFORCE_VULN_THRESHOLDS',
            defaultValue: true,
            description: 'Fail the pipeline when a deployable exceeds its vulnerability threshold'
        )
        string(
            name: 'AWS_REGION',
            defaultValue: 'us-east-1',
            description: 'AWS region for artifact and image publication'
        )
        string(
            name: 'AWS_ACCOUNT_ID',
            defaultValue: '',
            description: 'Optional AWS account override. Leave blank to resolve from the active AWS credentials.'
        )
    }

    // -------------------------------------------------------------------------
    // Environment — all cache lives on the agent filesystem, no S3 cache.
    // -------------------------------------------------------------------------
    environment {
        RUSTUP_HOME = '/var/cache/jenkins/rustup'
        CARGO_HOME  = '/var/cache/jenkins/cargo'

        // Per-project target dirs — prevents cargo's file lock from serializing
        // the two parallel builds. Registry + git deps are still shared (read-only
        // after first fetch, so safe to share).
        CARGO_TARGET_DIR_ENSURE  = '/var/cache/jenkins/cargo-target-ensure-cloud'
        CARGO_TARGET_DIR_SENTRICS = '/var/cache/jenkins/cargo-target-sentrics-core'

        CARGO_BUILD_JOBS                    = '4'
        CARGO_REGISTRIES_CRATES_IO_PROTOCOL = 'sparse'

        SQLX_OFFLINE = 'true'

        ARTIFACT_BUCKET = 'sentrics-ensure-lambda-artifacts-truststore'
    }

    stages {

        // =====================================================================
        // STAGE 1 — Resolve AWS publication target
        // Region is parameter-driven; account defaults to the active AWS caller.
        // =====================================================================
        stage('Resolve AWS Context') {
            steps {
                script {
                    env.AWS_REGION = params.AWS_REGION?.trim()
                    if (!env.AWS_REGION) {
                        error('AWS_REGION is required')
                    }

                    def accountOverride = params.AWS_ACCOUNT_ID?.trim()
                    if (accountOverride) {
                        if (!(accountOverride ==~ /\d{12}/)) {
                            error("AWS_ACCOUNT_ID must be a 12-digit AWS account ID: '${accountOverride}'")
                        }
                        env.AWS_ACCOUNT_ID = accountOverride
                    } else {
                        env.AWS_ACCOUNT_ID = sh(
                            script: 'aws sts get-caller-identity --query Account --output text',
                            returnStdout: true
                        ).trim()
                        if (!(env.AWS_ACCOUNT_ID ==~ /\d{12}/)) {
                            error("Unable to resolve a valid AWS account ID from the active AWS credentials: '${env.AWS_ACCOUNT_ID}'")
                        }
                    }

                    env.ECR_REGISTRY = "${env.AWS_ACCOUNT_ID}.dkr.ecr.${env.AWS_REGION}.amazonaws.com"

                    echo "AWS region      : ${env.AWS_REGION}"
                    echo "AWS account     : ${env.AWS_ACCOUNT_ID}"
                    echo "ECR registry    : ${env.ECR_REGISTRY}"
                    echo "Artifact bucket : ${env.ARTIFACT_BUCKET}"
                }
            }
        }

        // =====================================================================
        // STAGE 1 — Toolchain and prerequisite validation
        // =====================================================================
        stage('Verify Toolchain') {
            steps {
                sh '''#!/usr/bin/env bash
                    set -euo pipefail
                    export PATH="${CARGO_HOME}/bin:/usr/local/bin:${PATH}"

                    ensure_cache_dir() {
                        local dir="$1"
                        if ! mkdir -p "${dir}" 2>/dev/null; then
                            cat <<EOF
ERROR: unable to create required cache directory: ${dir}

Run this once on the Jenkins agent as an administrator:
  sudo mkdir -p /var/cache/jenkins/rustup
  sudo mkdir -p /var/cache/jenkins/cargo
  sudo mkdir -p /var/cache/jenkins/cargo-target-ensure-cloud
  sudo mkdir -p /var/cache/jenkins/cargo-target-sentrics-core
  sudo chown -R jenkins:jenkins /var/cache/jenkins
EOF
                            exit 1
                        fi

                        if [ ! -w "${dir}" ]; then
                            cat <<EOF
ERROR: cache directory is not writable: ${dir}

Run this once on the Jenkins agent as an administrator:
  sudo mkdir -p /var/cache/jenkins/rustup
  sudo mkdir -p /var/cache/jenkins/cargo
  sudo mkdir -p /var/cache/jenkins/cargo-target-ensure-cloud
  sudo mkdir -p /var/cache/jenkins/cargo-target-sentrics-core
  sudo chown -R jenkins:jenkins /var/cache/jenkins
EOF
                            exit 1
                        fi
                    }

                    echo "=== Verifying cache directories ==="
                    ensure_cache_dir "${RUSTUP_HOME}"
                    ensure_cache_dir "${CARGO_HOME}"
                    ensure_cache_dir "${CARGO_TARGET_DIR_ENSURE}"
                    ensure_cache_dir "${CARGO_TARGET_DIR_SENTRICS}"

                    require_tool() {
                        local tool="$1"
                        command -v "${tool}" >/dev/null 2>&1 || {
                            echo "ERROR: required tool not found: ${tool}"
                            exit 1
                        }
                    }

                    echo "=== Verifying required tools ==="
                    for tool in bash docker jq python3 trivy aws unzip gzip rustup cargo; do
                        require_tool "${tool}"
                    done

                    bash --version | sed -n '1p'
                    docker --version
                    jq --version
                    python3 --version
                    trivy --version
                    aws --version
                    unzip -v | sed -n '1p'
                    gzip --version 2>&1 | sed -n '1p'

                    verify_rust_repo() {
                        local repo="$1"
                        local rust_version
                        local cargo_lambda_version

                        rust_version="$(bash "./${repo}/scripts/ci/rust-version.sh")"
                        cargo_lambda_version="$(bash "./${repo}/scripts/ci/cargo-lambda-version.sh")"

                        echo "=== Installing / verifying Rust ${rust_version} for ${repo} ==="
                        rustup toolchain install "${rust_version}" \
                            --profile minimal \
                            --component rustfmt \
                            --component clippy
                        (
                            cd "./${repo}"
                            rustup override set "${rust_version}"
                            rustc --version | grep -q " ${rust_version} " || {
                                echo "ERROR: rustc version mismatch for ${repo} (expected ${rust_version})"
                                exit 1
                            }
                            cargo --version | grep -q " ${rust_version} " || {
                                echo "ERROR: cargo version mismatch for ${repo} (expected ${rust_version})"
                                exit 1
                            }
                        )

                        echo "=== Installing / verifying cargo-lambda ${cargo_lambda_version} for ${repo} ==="
                        if ! cargo lambda --version 2>/dev/null | grep -q " ${cargo_lambda_version} "; then
                            cargo install cargo-lambda --locked --version "${cargo_lambda_version}"
                        fi
                    }

                    verify_rust_repo ensure-cloud
                    verify_rust_repo sentrics-core

                    echo "=== Toolchain OK ==="
                    rustc --version
                    cargo --version
                    cargo lambda --version
                '''
            }
        }

        // =====================================================================
        // STAGE 2 — Lambda builds (parallel)
        // =====================================================================
        stage('Build Lambdas') {
            parallel {

                stage('ensure-cloud Lambdas') {
                    steps {
                        dir('ensure-cloud') {
                            sh '''
                                export PATH="${CARGO_HOME}/bin:/usr/local/bin:${PATH}"
                                export CARGO_TARGET_DIR="${CARGO_TARGET_DIR_ENSURE}"
                                COMMIT_HASH="$(echo "${RELEASE_SHA:-${GIT_COMMIT}}" | cut -c1-12)"

                                echo "=== Cache size (ensure-cloud) ==="
                                du -sh "${CARGO_TARGET_DIR}/" 2>/dev/null || echo "cold"

                                echo "Building headend-api..."
                                cd headend-api
                                cargo lambda build --release --output-format zip --bin headend-api
                                cp "${CARGO_TARGET_DIR}/lambda/headend-api/bootstrap.zip" ../headend-api.zip

                                echo "Building core-change-publisher..."
                                cd ../core-change-publisher
                                cargo lambda build --release --output-format zip --bin core-change-publisher
                                cp "${CARGO_TARGET_DIR}/lambda/core-change-publisher/bootstrap.zip" \
                                    ../core-change-publisher.zip

                                cd ..
                                mkdir -p out
                                mv headend-api.zip           out/headend-api.zip
                                mv core-change-publisher.zip out/core-change-publisher.zip

                                echo "=== ensure-cloud Lambda build complete ==="
                                ls -lh out/
                            '''
                        }
                    }
                }

                stage('sentrics-core Lambdas') {
                    steps {
                        dir('sentrics-core') {
                            sh '''
                                export PATH="${CARGO_HOME}/bin:/usr/local/bin:${PATH}"
                                export CARGO_TARGET_DIR="${CARGO_TARGET_DIR_SENTRICS}"
                                COMMIT_HASH="$(echo "${RELEASE_SHA:-${GIT_COMMIT}}" | cut -c1-12)"

                                echo "=== Cache size (sentrics-core) ==="
                                du -sh "${CARGO_TARGET_DIR}/" 2>/dev/null || echo "cold"

                                echo "Building resources-api, migrate, and resources-change-logger..."
                                SQLX_OFFLINE=true cargo lambda build --release \
                                    --output-format zip --compiler cargo \
                                    --bin resources-api \
                                    --bin migrate \
                                    --bin resources-change-logger

                                mkdir -p out
                                cp "${CARGO_TARGET_DIR}/lambda/resources-api/bootstrap.zip"           out/resources-api.zip
                                cp "${CARGO_TARGET_DIR}/lambda/migrate/bootstrap.zip"                 out/migrate.zip
                                cp "${CARGO_TARGET_DIR}/lambda/resources-change-logger/bootstrap.zip" out/resources-change-logger.zip

                                echo "=== sentrics-core Lambda build complete ==="
                                ls -lh out/
                            '''
                        }
                    }
                }
            }
        }

        // =====================================================================
        // STAGE 3 — Docker image builds + export as tar.gz (parallel)
        // docker save produces a tar the Jenkins archiver can store.
        // =====================================================================
        stage('Build Docker Images') {
            when {
                expression { return !params.SKIP_DOCKER_BUILDS }
            }
            parallel {

                stage('headend-gateway') {
                    steps {
                        dir('ensure-cloud/headend-gateway') {
                            sh '''
                                docker build -t headend-gateway:"${GIT_COMMIT}" \
                                    -f infra/headend-gateway/Dockerfile .
                                mkdir -p "${WORKSPACE}/docker-out"
                                docker save headend-gateway:"${GIT_COMMIT}" \
                                    | gzip > "${WORKSPACE}/docker-out/headend-gateway.tar.gz"
                            '''
                        }
                    }
                }

                stage('pki-api') {
                    steps {
                        dir('ensure-cloud/pki') {
                            sh '''
                                docker build -t pki-api:"${GIT_COMMIT}" \
                                    -f infra/pki-api/Dockerfile .
                                mkdir -p "${WORKSPACE}/docker-out"
                                docker save pki-api:"${GIT_COMMIT}" \
                                    | gzip > "${WORKSPACE}/docker-out/pki-api.tar.gz"
                            '''
                        }
                    }
                }

                stage('stepca') {
                    steps {
                        dir('ensure-cloud/pki/infra/stepca') {
                            sh '''
                                docker build -t stepca:"${GIT_COMMIT}" .
                                mkdir -p "${WORKSPACE}/docker-out"
                                docker save stepca:"${GIT_COMMIT}" \
                                    | gzip > "${WORKSPACE}/docker-out/stepca.tar.gz"
                            '''
                        }
                    }
                }

                stage('yardi-sync') {
                    steps {
                        dir('sentrics-core/yardi-sync') {
                            sh '''
                                docker build -t yardi-sync:"${GIT_COMMIT}" \
                                    -f infra/yardi-sync/Dockerfile .
                                mkdir -p "${WORKSPACE}/docker-out"
                                docker save yardi-sync:"${GIT_COMMIT}" \
                                    | gzip > "${WORKSPACE}/docker-out/yardi-sync.tar.gz"
                            '''
                        }
                    }
                }
            }
        }

        // =====================================================================
        // STAGE 4 — Trivy scan
        // Matches the security buildspec model:
        //   • fail on missing artifacts or Trivy scan errors
        //   • count severities from JSON with jq
        //   • record per-deployable status and notes in one summary file
        // =====================================================================
        stage('Trivy Scan') {
            steps {
                sh '''#!/usr/bin/env bash
                    set -euo pipefail

                    mkdir -p trivy-reports
                    SUMMARY_FILE="trivy-reports/security-scan-summary.tsv"
                    LAMBDA_REPORT="trivy-reports/trivy-lambda-scan.txt"
                    DOCKER_REPORT="trivy-reports/trivy-docker-scan.txt"
                    : > "${LAMBDA_REPORT}"
                    : > "${DOCKER_REPORT}"
                    printf 'TYPE\tDEPLOYABLE\tCRITICAL\tHIGH\tSTATUS\tNOTE\n' > "${SUMMARY_FILE}"

                    FAILED_DEPLOYABLES=""

                    add_failure() {
                        local deployable="$1"
                        if [ -z "${FAILED_DEPLOYABLES}" ]; then
                            FAILED_DEPLOYABLES="${deployable}"
                        else
                            FAILED_DEPLOYABLES="${FAILED_DEPLOYABLES},${deployable}"
                        fi
                    }

                    add_summary() {
                        local type="$1"
                        local deployable="$2"
                        local critical="$3"
                        local high="$4"
                        local status="$5"
                        local note="$6"
                        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
                            "${type}" "${deployable}" "${critical}" "${high}" "${status}" "${note}" >> "${SUMMARY_FILE}"
                    }

                    count_severity() {
                        local report_file="$1"
                        local sev="$2"
                        jq --arg sev "$sev" '[.. | objects | select(has("Severity")) | .Severity | select(. == $sev)] | length' "${report_file}"
                    }

                    case ",${TRIVY_SEVERITY}," in
                        *,HIGH,*|*,CRITICAL,*)
                            ;;
                        *)
                            echo "ERROR: TRIVY_SEVERITY must include HIGH and/or CRITICAL for this gate. Got '${TRIVY_SEVERITY}'"
                            exit 1
                            ;;
                    esac

                    resolve_image_thresholds() {
                        local deployable="$1"
                        local max_critical="${MAX_CRITICAL}"
                        local max_high="${MAX_HIGH}"
                        case "${deployable}" in
                            stepca)
                                max_critical="${STEPCA_MAX_CRITICAL:-${MAX_CRITICAL}}"
                                max_high="${STEPCA_MAX_HIGH:-${MAX_HIGH}}"
                                ;;
                        esac
                        echo "${max_critical} ${max_high}"
                    }

                    print_vuln_details() {
                        local report_file="$1"
                        local artifact_type="$2"
                        local deployable="$3"
                        local critical_count="$4"
                        local high_count="$5"
                        local detail_file="trivy-reports/detail-${artifact_type}-${deployable}.txt"

                        rm -f "${detail_file}"
                        if [ "${critical_count}" -eq 0 ] && [ "${high_count}" -eq 0 ]; then
                            return
                        fi

                        jq -r '
                            [
                              .Results[]?
                              | .Vulnerabilities[]?
                              | select(.Severity == "CRITICAL" or .Severity == "HIGH")
                              | [
                                  .Severity,
                                  .VulnerabilityID,
                                  (.PkgName // "-"),
                                  (.InstalledVersion // "-"),
                                  (.FixedVersion // "-"),
                                  ((.Title // .Description // "-") | gsub("[\\r\\n\\t]+"; " ") | .[0:140])
                                ]
                            ]
                            | sort_by(.[0], .[1])
                            | .[]
                            | @tsv
                        ' "${report_file}" | while IFS=$'\t' read -r sev vuln_id pkg installed fixed title; do
                            printf '  - %s %s pkg=%s installed=%s fixed=%s title=%s\n' \
                                "${sev}" "${vuln_id}" "${pkg}" "${installed}" "${fixed}" "${title}"
                        done > "${detail_file}"
                    }

                    process_lambda_bundle() {
                        local manifest_path="trivy-reports/lambda-manifest.json"
                        local artifact_dir="trivy-reports/lambda-bundle"
                        local item

                        mkdir -p "${artifact_dir}"
                        jq -n '{
                            artifacts: [
                                { deployable: "headend-api", file: "headend-api.zip", path: "ensure-cloud/out/headend-api.zip" },
                                { deployable: "core-change-publisher", file: "core-change-publisher.zip", path: "ensure-cloud/out/core-change-publisher.zip" },
                                { deployable: "resources-api", file: "resources-api.zip", path: "sentrics-core/out/resources-api.zip" },
                                { deployable: "migrate", file: "migrate.zip", path: "sentrics-core/out/migrate.zip" },
                                { deployable: "resources-change-logger", file: "resources-change-logger.zip", path: "sentrics-core/out/resources-change-logger.zip" }
                            ]
                        }' > "${manifest_path}"

                        jq -e '.artifacts and (.artifacts | type == "array")' "${manifest_path}" >/dev/null 2>&1 || {
                            echo "ERROR: Invalid lambda manifest format in ${manifest_path}"
                            add_summary "lambda" "bundle" "-" "-" "ERROR" "invalid lambda-manifest.json format"
                            add_failure "lambda-bundle"
                            return
                        }

                        while IFS= read -r item; do
                            local deployable
                            local zip_path
                            local extract_dir
                            local report_file
                            local critical_count
                            local high_count

                            deployable="$(echo "${item}" | jq -r '.deployable')"
                            zip_path="$(echo "${item}" | jq -r '.path')"
                            extract_dir="${artifact_dir}/unzip-${deployable}"
                            report_file="trivy-reports/trivy-lambda-${deployable}.json"

                            process_lambda_artifact "${deployable}" "${zip_path}" "${extract_dir}" "${report_file}"
                        done < <(jq -c '.artifacts[]' "${manifest_path}")
                    }

                    process_lambda_artifact() {
                        local deployable="$1"
                        local zip_path="$2"
                        local extract_dir="$3"
                        local report_file="$4"
                        local critical_count
                        local high_count

                        if [ ! -f "${zip_path}" ]; then
                            echo "ERROR: Missing lambda artifact ${zip_path}"
                            add_summary "lambda" "${deployable}" "-" "-" "ERROR" "missing lambda zip"
                            add_failure "${deployable}"
                            return
                        fi

                        echo "=== Scanning lambda deployable ${deployable} (${zip_path}) ===" | tee -a "${LAMBDA_REPORT}"
                        rm -rf "${extract_dir}"
                        mkdir -p "${extract_dir}"
                        unzip -q "${zip_path}" -d "${extract_dir}"
                        [ -f "${extract_dir}/bootstrap" ] || {
                            echo "ERROR: ${deployable} zip did not contain bootstrap"
                            add_summary "lambda" "${deployable}" "-" "-" "ERROR" "zip missing bootstrap"
                            add_failure "${deployable}"
                            return
                        }

                        if ! trivy fs --quiet --no-progress --scanners vuln --severity "${TRIVY_SEVERITY}" --format json --output "${report_file}" "${extract_dir}"; then
                            echo "ERROR: Trivy fs scan failed for ${deployable}"
                            add_summary "lambda" "${deployable}" "-" "-" "ERROR" "trivy fs scan failed"
                            add_failure "${deployable}"
                            return
                        fi

                        critical_count="$(count_severity "${report_file}" "CRITICAL")"
                        high_count="$(count_severity "${report_file}" "HIGH")"
                        echo "${deployable}: CRITICAL=${critical_count}, HIGH=${high_count}" | tee -a "${LAMBDA_REPORT}"
                        print_vuln_details "${report_file}" "lambda" "${deployable}" "${critical_count}" "${high_count}"

                        if [ "${ENFORCE_VULN_THRESHOLDS}" = "true" ] && { [ "${critical_count}" -gt "${MAX_CRITICAL}" ] || [ "${high_count}" -gt "${MAX_HIGH}" ]; }; then
                            add_summary "lambda" "${deployable}" "${critical_count}" "${high_count}" "BLOCKED" "threshold exceeded"
                            add_failure "${deployable}"
                            return
                        fi

                        add_summary "lambda" "${deployable}" "${critical_count}" "${high_count}" "PASSED" "eligible for S3 publish"
                    }

                    process_image_artifact() {
                        local deployable="$1"
                        local image_ref="$2"
                        local report_file="trivy-reports/trivy-image-${deployable}.json"
                        local critical_count
                        local high_count
                        local thresholds
                        local allowed_max_critical
                        local allowed_max_high

                        if ! docker image inspect "${image_ref}" >/dev/null 2>&1; then
                            echo "ERROR: Missing Docker image ${image_ref}"
                            add_summary "image" "${deployable}" "-" "-" "ERROR" "docker image missing"
                            add_failure "${deployable}"
                            return
                        fi

                        echo "=== Scanning image deployable ${deployable} (${image_ref}) ===" | tee -a "${DOCKER_REPORT}"
                        if ! trivy image --quiet --no-progress --scanners vuln --severity "${TRIVY_SEVERITY}" --format json --output "${report_file}" "${image_ref}"; then
                            echo "ERROR: Trivy image scan failed for ${deployable}"
                            add_summary "image" "${deployable}" "-" "-" "ERROR" "trivy image scan failed"
                            add_failure "${deployable}"
                            return
                        fi

                        critical_count="$(count_severity "${report_file}" "CRITICAL")"
                        high_count="$(count_severity "${report_file}" "HIGH")"
                        thresholds="$(resolve_image_thresholds "${deployable}")"
                        allowed_max_critical="${thresholds%% *}"
                        allowed_max_high="${thresholds##* }"
                        echo "${deployable}: CRITICAL=${critical_count}, HIGH=${high_count}" | tee -a "${DOCKER_REPORT}"
                        print_vuln_details "${report_file}" "image" "${deployable}" "${critical_count}" "${high_count}"

                        if [ "${ENFORCE_VULN_THRESHOLDS}" = "true" ] && { [ "${critical_count}" -gt "${allowed_max_critical}" ] || [ "${high_count}" -gt "${allowed_max_high}" ]; }; then
                            add_summary "image" "${deployable}" "${critical_count}" "${high_count}" "BLOCKED" "threshold exceeded (max C=${allowed_max_critical}, H=${allowed_max_high})"
                            add_failure "${deployable}"
                            return
                        fi

                        add_summary "image" "${deployable}" "${critical_count}" "${high_count}" "PASSED" "eligible for ECR publish"
                    }

                    process_lambda_bundle

                    if [ "${SKIP_DOCKER_BUILDS}" = "true" ]; then
                        add_summary "image" "headend-gateway" "-" "-" "SKIPPED" "docker builds skipped"
                        add_summary "image" "pki-api" "-" "-" "SKIPPED" "docker builds skipped"
                        add_summary "image" "stepca" "-" "-" "SKIPPED" "docker builds skipped"
                        add_summary "image" "yardi-sync" "-" "-" "SKIPPED" "docker builds skipped"
                    else
                        process_image_artifact "headend-gateway" "headend-gateway:${GIT_COMMIT}"
                        process_image_artifact "pki-api" "pki-api:${GIT_COMMIT}"
                        process_image_artifact "stepca" "stepca:${GIT_COMMIT}"
                        process_image_artifact "yardi-sync" "yardi-sync:${GIT_COMMIT}"
                    fi

                    if [ -n "${FAILED_DEPLOYABLES}" ]; then
                        echo "BLOCKED_DEPLOYABLES=${FAILED_DEPLOYABLES} — details will be reported in CVE Gate."
                    else
                        echo "All deployables passed security scan and are eligible for publish."
                    fi
                '''
            }
        }

        // =====================================================================
        // STAGE 5 — Archive everything in Jenkins
        //   • Lambda zips        : ensure-cloud/out/  + sentrics-core/out/
        //   • Docker image tars  : docker-out/
        //   • Trivy scan reports : trivy-reports/
        // =====================================================================
        stage('Archive Artifacts') {
            steps {
                archiveArtifacts artifacts: 'ensure-cloud/out/**/*',   allowEmptyArchive: false
                archiveArtifacts artifacts: 'sentrics-core/out/**/*',  allowEmptyArchive: false
                archiveArtifacts artifacts: 'trivy-reports/**/*',      allowEmptyArchive: true
                script {
                    if (!params.SKIP_DOCKER_BUILDS) {
                        archiveArtifacts artifacts: 'docker-out/**/*', allowEmptyArchive: false
                    }
                }
            }
        }

        // =====================================================================
        // STAGE 6 — CVE Gate
        // Prints the per-artifact scan summary and the full CVE detail tables
        // for any findings.  Fails (blocking publish) when the total
        // CRITICAL or HIGH count exceeds the MAX_CRITICAL / MAX_HIGH
        // thresholds set in the build parameters.
        // =====================================================================
        stage('CVE Gate') {
            steps {
                sh '''#!/usr/bin/env bash
                    set -euo pipefail
                    SUMMARY_FILE="trivy-reports/security-scan-summary.tsv"
                    [ -f "${SUMMARY_FILE}" ] || { echo "ERROR: Missing ${SUMMARY_FILE}"; exit 1; }

                    echo ""
                    echo "=== Security Scan Findings Summary ==="
                    if command -v column >/dev/null 2>&1; then
                        column -t -s $'\t' "${SUMMARY_FILE}"
                    else
                        cat "${SUMMARY_FILE}"
                    fi

                    # Print CVE details + fix information for every deployable with findings
                    HAS_DETAIL=0
                    for DETAIL in trivy-reports/detail-*.txt; do
                        [ -f "${DETAIL}" ] || continue
                        [ -s "${DETAIL}" ]  || continue
                        if [ "${HAS_DETAIL}" -eq 0 ]; then
                            echo ""
                            echo "=== CVE Details & Available Fixes ==="
                            HAS_DETAIL=1
                        fi
                        # Derive a clean label from the filename (detail-image-stepca.txt → image/stepca)
                        LABEL="${DETAIL#trivy-reports/detail-}"
                        LABEL="${LABEL%.txt}"
                        LABEL="${LABEL/-//}"
                        echo ""
                        echo "--- ${LABEL} ---"
                        # Header
                        printf '  %-10s %-20s %-30s %-18s %-18s %s\n' \
                            "SEVERITY" "CVE" "PACKAGE" "INSTALLED" "FIXED" "TITLE"
                        cat "${DETAIL}"
                        echo ""
                    done

                    if [ "${HAS_DETAIL}" -eq 0 ]; then
                        echo "No CRITICAL/HIGH findings."
                    fi

                    # Fail here if any deployable was marked BLOCKED by the scan
                    BLOCKED=$(awk -F'\t' 'NR > 1 && $5 == "BLOCKED" { printf "%s ", $2 }' "${SUMMARY_FILE}")
                    if [ -n "${BLOCKED}" ]; then
                        echo ""
                        echo "CVE GATE FAILED — blocked deployables: ${BLOCKED}"
                        echo "Fix the vulnerabilities or raise the thresholds in Build Parameters, then re-run."
                        exit 1
                    fi

                    echo ""
                    echo "CVE gate passed — all deployables cleared for publish."
                '''
            }
        }

        // =====================================================================
        // STAGE 7 — Publish Lambda zips to S3, Docker images to ECR, manifest
        // =====================================================================
        stage('Publish to S3 & ECR') {
            steps {
                sh '''
                    SHA="${RELEASE_SHA:-${GIT_COMMIT}}"

                    echo "=== Pushing Lambda zips to S3 ==="
                    aws s3 cp ensure-cloud/out/headend-api.zip \
                        "s3://${ARTIFACT_BUCKET}/lambda-artifacts/headend-api/headend-api-${SHA}.zip"
                    aws s3 cp ensure-cloud/out/core-change-publisher.zip \
                        "s3://${ARTIFACT_BUCKET}/lambda-artifacts/core-change-publisher/core-change-publisher-${SHA}.zip"
                    aws s3 cp sentrics-core/out/resources-api.zip \
                        "s3://${ARTIFACT_BUCKET}/lambda-artifacts/resources-api/resources-api-${SHA}.zip"
                    aws s3 cp sentrics-core/out/migrate.zip \
                        "s3://${ARTIFACT_BUCKET}/lambda-artifacts/migrate/migrate-${SHA}.zip"
                    aws s3 cp sentrics-core/out/resources-change-logger.zip \
                        "s3://${ARTIFACT_BUCKET}/lambda-artifacts/resources-change-logger/resources-change-logger-${SHA}.zip"
                    echo "=== Lambda zips pushed ==="
                '''
                script {
                    if (!params.SKIP_DOCKER_BUILDS) {
                        sh '''
                            SHA="${RELEASE_SHA:-${GIT_COMMIT}}"

                            echo "=== Logging in to ECR ==="
                            aws ecr get-login-password --region "${AWS_REGION}" \
                                | docker login --username AWS --password-stdin "${ECR_REGISTRY}"

                            echo "=== Pushing Docker images to ECR ==="
                            for SERVICE in headend-gateway pki-api stepca; do
                                REPO="${ECR_REGISTRY}/ensure-cloud-${SERVICE}"
                                docker tag "${SERVICE}:${GIT_COMMIT}" "${REPO}:${SHA}"
                                docker push "${REPO}:${SHA}"
                                echo "Pushed ${REPO}:${SHA}"
                            done

                            docker tag "yardi-sync:${GIT_COMMIT}" \
                                "${ECR_REGISTRY}/sentrics-core-yardi-sync-repo:${SHA}"
                            docker push "${ECR_REGISTRY}/sentrics-core-yardi-sync-repo:${SHA}"
                            echo "Pushed ${ECR_REGISTRY}/sentrics-core-yardi-sync-repo:${SHA}"
                            echo "=== Docker images pushed ==="
                        '''
                    }
                }
                sh '''
                    SHA="${RELEASE_SHA:-${GIT_COMMIT}}"
                    BUILT_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

                    echo "=== Writing manifest.json ==="
                    jq -n \
                        --arg commit  "${SHA}" \
                        --arg built   "${BUILT_AT}" \
                        --arg bucket  "${ARTIFACT_BUCKET}" \
                        --arg ecr     "${ECR_REGISTRY}" \
                        --argjson skip_docker "${SKIP_DOCKER_BUILDS}" \
                        '{
                            commit:       $commit,
                            built_at:     $built,
                            skip_docker:  $skip_docker,
                            lambdas: {
                                "headend-api":             ("s3://" + $bucket + "/lambda-artifacts/headend-api/headend-api-"                         + $commit + ".zip"),
                                "core-change-publisher":   ("s3://" + $bucket + "/lambda-artifacts/core-change-publisher/core-change-publisher-"     + $commit + ".zip"),
                                "resources-api":           ("s3://" + $bucket + "/lambda-artifacts/resources-api/resources-api-"                     + $commit + ".zip"),
                                "migrate":                 ("s3://" + $bucket + "/lambda-artifacts/migrate/migrate-"                                 + $commit + ".zip"),
                                "resources-change-logger": ("s3://" + $bucket + "/lambda-artifacts/resources-change-logger/resources-change-logger-" + $commit + ".zip")
                            },
                            images: {
                                "headend-gateway": ($ecr + "/ensure-cloud-headend-gateway:"        + $commit),
                                "pki-api":         ($ecr + "/ensure-cloud-pki-api:"                + $commit),
                                "stepca":          ($ecr + "/ensure-cloud-stepca:"                 + $commit),
                                "yardi-sync":      ($ecr + "/sentrics-core-yardi-sync-repo:"       + $commit)
                            }
                        }' > /tmp/manifest.json

                    aws s3 cp /tmp/manifest.json \
                        "s3://${ARTIFACT_BUCKET}/builds/${SHA}/manifest.json"
                    echo "=== Manifest published: s3://${ARTIFACT_BUCKET}/builds/${SHA}/manifest.json ==="
                '''
            }
        }

        // =====================================================================
        // STAGE 9 — Cache stats
        // =====================================================================
        stage('Cache Stats') {
            steps {
                sh '''
                    echo "=== CARGO CACHE STATS ==="
                    echo "ensure-cloud target  : $(du -sh "${CARGO_TARGET_DIR_ENSURE}/" 2>/dev/null || echo empty)"
                    echo "sentrics-core target : $(du -sh "${CARGO_TARGET_DIR_SENTRICS}/" 2>/dev/null || echo empty)"
                    echo "Registry             : $(du -sh "${CARGO_HOME}/registry/" 2>/dev/null || echo empty)"
                    echo "Git deps             : $(du -sh "${CARGO_HOME}/git/" 2>/dev/null || echo empty)"
                    echo "rlib count (ensure)  : $(find "${CARGO_TARGET_DIR_ENSURE}/" -name "*.rlib" 2>/dev/null | wc -l)"
                    echo "rlib count (sentrics): $(find "${CARGO_TARGET_DIR_SENTRICS}/" -name "*.rlib" 2>/dev/null | wc -l)"
                    echo "========================="
                '''
            }
        }
    }

    // =========================================================================
    // Post-pipeline actions
    // =========================================================================
    post {
        success {
            echo "Pipeline PASSED — artifacts published for ${GIT_COMMIT}."
        }
        failure {
            echo "Pipeline FAILED — check stage logs above."
        }
        always {
            sh 'docker image prune -f || true'
            cleanWs()
        }
    }
}
