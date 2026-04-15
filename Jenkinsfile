// =============================================================================
// Sentrics Event-Driven Platform — Jenkins Pipeline  (speed-test mode)
//
//   1. Verify toolchain versions (fail fast on version drift)
//   2. Parallel Lambda builds  : ensure-cloud Lambdas | sentrics-core Lambdas
//   3. Parallel Docker builds  : headend-gateway | pki-api | stepca | yardi-sync
//      └─ exports each image as a .tar.gz for archiving
//   4. Trivy scan (informational — does NOT break the pipeline)
//      ├─ Lambda zip artifacts  → trivy-lambda-scan.txt
//      └─ Docker images         → trivy-docker-scan.txt
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
            description: 'Severity levels to report in Trivy scan (informational only)'
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

                                echo "Building resources-api and migrate..."
                                cd resources-api
                                SQLX_OFFLINE=true cargo lambda build --release \
                                    --output-format zip --compiler cargo --bin resources-api
                                SQLX_OFFLINE=true cargo lambda build --release \
                                    --output-format zip --compiler cargo --bin migrate
                                cp "${CARGO_TARGET_DIR}/lambda/resources-api/bootstrap.zip" ../resources-api.zip
                                cp "${CARGO_TARGET_DIR}/lambda/migrate/bootstrap.zip"        ../migrate.zip

                                echo "Building resources-change-logger..."
                                cd ../resources-change-logger
                                cargo lambda build --release --output-format zip \
                                    --bin resources-change-logger
                                cp "${CARGO_TARGET_DIR}/lambda/resources-change-logger/bootstrap.zip" \
                                    ../resources-change-logger.zip

                                cd ..
                                mkdir -p out
                                mv resources-api.zip           out/resources-api.zip
                                mv migrate.zip                 out/migrate.zip
                                mv resources-change-logger.zip out/resources-change-logger.zip

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
        // STAGE 4 — Trivy scan (informational — exit-code 0, never breaks build)
        // Results are written to text files and archived alongside the artifacts.
        // =====================================================================
        stage('Trivy Scan') {
            parallel {

                stage('Scan Lambda Zips') {
                    steps {
                        sh '''
                            mkdir -p trivy-reports
                            REPORT="trivy-reports/trivy-lambda-scan.txt"
                            : > "${REPORT}"

                            for ZIP in \
                                ensure-cloud/out/headend-api.zip \
                                ensure-cloud/out/core-change-publisher.zip \
                                sentrics-core/out/resources-api.zip \
                                sentrics-core/out/migrate.zip \
                                sentrics-core/out/resources-change-logger.zip
                            do
                                echo "===== ${ZIP} =====" | tee -a "${REPORT}"
                                trivy fs \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format table \
                                    "${ZIP}" 2>&1 | tee -a "${REPORT}"
                                echo "" >> "${REPORT}"
                            done

                            echo "=== Lambda scan complete (informational) ==="
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
                            : > "${REPORT}"

                            for IMAGE in \
                                headend-gateway:"${GIT_COMMIT}" \
                                pki-api:"${GIT_COMMIT}" \
                                stepca:"${GIT_COMMIT}" \
                                yardi-sync:"${GIT_COMMIT}"
                            do
                                echo "===== ${IMAGE} =====" | tee -a "${REPORT}"
                                trivy image \
                                    --severity "${TRIVY_SEVERITY}" \
                                    --exit-code 0 \
                                    --no-progress \
                                    --format table \
                                    "${IMAGE}" 2>&1 | tee -a "${REPORT}"
                                echo "" >> "${REPORT}"
                            done

                            echo "=== Docker scan complete (informational) ==="
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
        // STAGE 6 — Cache stats
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
            echo "Pipeline PASSED — all artifacts archived in Jenkins for build ${GIT_COMMIT}."
        }
        failure {
            echo "Pipeline FAILED — check stage logs above."
        }
        always {
            sh 'docker image prune -f || true'
        }
    }
}
