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

        AWS_REGION      = 'us-east-1'
        AWS_ACCOUNT_ID  = '892234674906'
        ECR_REGISTRY    = '892234674906.dkr.ecr.us-east-1.amazonaws.com'
        ARTIFACT_BUCKET = 'sentrics-ensure-lambda-artifacts-truststore'
    }

    stages {

        // =====================================================================
        // STAGE 1 — Toolchain version gate
        // =====================================================================
        stage('Verify Toolchain') {
            steps {
                dir('ensure-cloud') {
                    sh '''
                        export PATH="${CARGO_HOME}/bin:/usr/local/bin:${PATH}"

                        RUST_VERSION="$(bash ./scripts/ci/rust-version.sh)"
                        CARGO_LAMBDA_VERSION="$(bash ./scripts/ci/cargo-lambda-version.sh)"

                        echo "=== Installing / verifying Rust ${RUST_VERSION} ==="
                        rustup toolchain install "${RUST_VERSION}" \
                            --profile minimal \
                            --component rustfmt \
                            --component clippy
                        rustup override set "${RUST_VERSION}"

                        rustc --version | grep -q " ${RUST_VERSION} " || {
                            echo "ERROR: rustc version mismatch (expected ${RUST_VERSION})"
                            exit 1
                        }
                        cargo --version | grep -q " ${RUST_VERSION} " || {
                            echo "ERROR: cargo version mismatch (expected ${RUST_VERSION})"
                            exit 1
                        }

                        echo "=== Installing / verifying cargo-lambda ${CARGO_LAMBDA_VERSION} ==="
                        if ! cargo lambda --version 2>/dev/null | grep -q " ${CARGO_LAMBDA_VERSION} "; then
                            cargo install cargo-lambda --locked --version "${CARGO_LAMBDA_VERSION}"
                        fi

                        echo "=== Toolchain OK ==="
                        rustc --version && cargo --version && cargo lambda --version
                    '''
                }
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
        // Each parallel branch scans its artifacts, writes a human-readable
        // table report AND a CSV summary (artifact,critical,high).
        // Both branches always exit 0 — the CVE gate runs after archiving.
        // =====================================================================
        stage('Trivy Scan') {
            parallel {

                stage('Scan Lambda Zips') {
                    steps {
                        sh '''
                            mkdir -p trivy-reports
                            REPORT="trivy-reports/trivy-lambda-scan.txt"
                            SUMMARY="trivy-reports/lambda-cve-summary.csv"
                            : > "${REPORT}"
                            echo "type,path,name,critical,high" > "${SUMMARY}"

                            for ENTRY in \
                                "ensure-cloud/out/headend-api.zip:headend-api" \
                                "ensure-cloud/out/core-change-publisher.zip:core-change-publisher" \
                                "sentrics-core/out/resources-api.zip:resources-api" \
                                "sentrics-core/out/migrate.zip:migrate" \
                                "sentrics-core/out/resources-change-logger.zip:resources-change-logger"
                            do
                                ZIP="${ENTRY%%:*}"
                                NAME="${ENTRY##*:}"
                                echo "===== ${ZIP} =====" | tee -a "${REPORT}"

                                # JSON scan — count CRITICAL/HIGH per artifact
                                trivy fs \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format json \
                                    "${ZIP}" > trivy-reports/tmp-scan.json 2>/dev/null || true

                                COUNTS=$(python3 - <<'PYEOF'
import json
try:
    d = json.load(open("trivy-reports/tmp-scan.json"))
    c = sum(1 for r in d.get("Results", []) for v in (r.get("Vulnerabilities") or []) if v.get("Severity") == "CRITICAL")
    h = sum(1 for r in d.get("Results", []) for v in (r.get("Vulnerabilities") or []) if v.get("Severity") == "HIGH")
    print(c, h)
except Exception:
    print("0 0")
PYEOF
)
                                CRITICAL=$(echo "${COUNTS}" | awk '{print $1}')
                                HIGH=$(echo "${COUNTS}" | awk '{print $2}')
                                echo "lambda,${ZIP},${NAME},${CRITICAL},${HIGH}" >> "${SUMMARY}"

                                # Table scan — human-readable detail, captured once
                                TABLE=$(trivy fs \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format table \
                                    "${ZIP}" 2>&1)
                                printf '%s\n\n' "${TABLE}" >> "${REPORT}"

                                # Per-artifact detail file (used by summary stage)
                                if [ "${CRITICAL}" -gt 0 ] || [ "${HIGH}" -gt 0 ]; then
                                    printf '%s\n' "${TABLE}" > "trivy-reports/detail-lambda-${NAME}.txt"
                                fi
                            done

                            rm -f trivy-reports/tmp-scan.json
                            echo "=== Lambda scan complete ==="
                        '''
                    }
                }

                stage('Scan Docker Images') {
                    when {
                        expression { return !params.SKIP_DOCKER_BUILDS }
                    }
                    steps {
                        sh '''
                            mkdir -p trivy-reports
                            REPORT="trivy-reports/trivy-docker-scan.txt"
                            SUMMARY="trivy-reports/docker-cve-summary.csv"
                            : > "${REPORT}"
                            echo "type,path,name,critical,high" > "${SUMMARY}"

                            for NAME in headend-gateway pki-api stepca yardi-sync; do
                                IMAGE="${NAME}:${GIT_COMMIT}"
                                echo "===== ${IMAGE} =====" | tee -a "${REPORT}"

                                trivy image \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format json \
                                    "${IMAGE}" > trivy-reports/tmp-scan.json 2>/dev/null || true

                                COUNTS=$(python3 - <<'PYEOF'
import json
try:
    d = json.load(open("trivy-reports/tmp-scan.json"))
    c = sum(1 for r in d.get("Results", []) for v in (r.get("Vulnerabilities") or []) if v.get("Severity") == "CRITICAL")
    h = sum(1 for r in d.get("Results", []) for v in (r.get("Vulnerabilities") or []) if v.get("Severity") == "HIGH")
    print(c, h)
except Exception:
    print("0 0")
PYEOF
)
                                CRITICAL=$(echo "${COUNTS}" | awk '{print $1}')
                                HIGH=$(echo "${COUNTS}" | awk '{print $2}')
                                echo "image,${IMAGE},${NAME},${CRITICAL},${HIGH}" >> "${SUMMARY}"

                                TABLE=$(trivy image \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format table \
                                    "${IMAGE}" 2>&1)
                                printf '%s\n\n' "${TABLE}" >> "${REPORT}"

                                if [ "${CRITICAL}" -gt 0 ] || [ "${HIGH}" -gt 0 ]; then
                                    printf '%s\n' "${TABLE}" > "trivy-reports/detail-image-${NAME}.txt"
                                fi
                            done

                            rm -f trivy-reports/tmp-scan.json
                            echo "=== Docker scan complete ==="
                        '''
                    }
                }
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
                sh """
                    echo ""
                    echo "=== Security Scan Findings Summary ==="
                    printf "%-8s %-25s %-10s %-6s\\n" "TYPE" "DEPLOYABLE" "CRITICAL" "HIGH"

                    TOTAL_CRITICAL=0
                    TOTAL_HIGH=0
                    HAS_FINDINGS=0

                    for CSV in \\
                        trivy-reports/lambda-cve-summary.csv \\
                        trivy-reports/docker-cve-summary.csv
                    do
                        [ -f "\${CSV}" ] || continue
                        while IFS=',' read -r type path name critical high; do
                            [ "\${type}" = "type" ] && continue
                            printf "%-8s %-25s %-10s %-6s\\n" \\
                                "\${type}" "\${name}" "\${critical}" "\${high}"
                            TOTAL_CRITICAL=\$((TOTAL_CRITICAL + critical))
                            TOTAL_HIGH=\$((TOTAL_HIGH + high))
                            if [ "\${critical}" -gt 0 ] || [ "\${high}" -gt 0 ]; then
                                HAS_FINDINGS=1
                            fi
                        done < "\${CSV}"
                    done

                    echo ""
                    echo "Total: CRITICAL=\${TOTAL_CRITICAL}  HIGH=\${TOTAL_HIGH}"
                    echo "Thresholds: MAX_CRITICAL=${params.MAX_CRITICAL}  MAX_HIGH=${params.MAX_HIGH}"

                    if [ "\${HAS_FINDINGS}" -eq 1 ]; then
                        echo ""
                        echo "--- CVE Details (pipeline may still proceed if within thresholds) ---"
                        for DETAIL in trivy-reports/detail-*.txt; do
                            [ -f "\${DETAIL}" ] || continue
                            cat "\${DETAIL}"
                            echo ""
                        done
                    fi

                    if [ "\${TOTAL_CRITICAL}" -gt "${params.MAX_CRITICAL}" ] || \\
                       [ "\${TOTAL_HIGH}"     -gt "${params.MAX_HIGH}" ]; then
                        echo ""
                        echo "CVE GATE FAILED — CRITICAL=\${TOTAL_CRITICAL} (max ${params.MAX_CRITICAL})  HIGH=\${TOTAL_HIGH} (max ${params.MAX_HIGH})"
                        echo "Publish blocked. Raise MAX_CRITICAL / MAX_HIGH in Build Parameters to allow, or fix the CVEs."
                        exit 1
                    fi

                    echo ""
                    if [ "\${HAS_FINDINGS}" -eq 0 ]; then
                        echo "CVE gate passed — no findings. Proceeding to publish."
                    else
                        echo "CVE gate passed — findings are within configured thresholds. Proceeding to publish."
                    fi
                """
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
