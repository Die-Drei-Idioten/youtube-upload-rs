pipeline {
    agent {
        dockerContainer {
            image 'rust:1.75'  // Use official Rust Docker image
            args '-v $HOME/.cargo:/root/.cargo'  // Cache Cargo dependencies
        }
    }

    triggers {
        // Trigger build on SCM changes (pushes)
        pollSCM('H/5 * * * *')  // Poll SCM every 5 minutes
    }

    environment {
        // Rust environment variables
        CARGO_HOME = '/root/.cargo'
        RUSTUP_HOME = '/root/.rustup'
        // Optional: Set specific Rust version
        // RUST_VERSION = '1.75.0'
    }

    stages {
        stage('Checkout') {
            steps {
                // Checkout source code
                checkout scm
            }
        }

        stage('Rust Info') {
            steps {
                // Display Rust and Cargo versions
                sh 'rustc --version'
                sh 'cargo --version'
            }
        }

        stage('Cache Dependencies') {
            steps {
                // Update Cargo index and cache dependencies
                sh 'cargo fetch'
            }
        }

        stage('Lint') {
            steps {
                // Run clippy for linting
                sh 'cargo clippy -- -D warnings'
            }
        }

        stage('Format Check') {
            steps {
                // Check code formatting
                sh 'cargo fmt -- --check'
            }
        }

        stage('Build') {
            steps {
                // Build the project
                sh 'cargo build --release'
            }
        }

        stage('Test') {
            steps {
                // Run tests
                sh 'cargo test --release'
            }
        }

        stage('Security Audit') {
            steps {
                // Optional: Run security audit
                script {
                    try {
                        sh 'cargo install cargo-audit || true'
                        sh 'cargo audit'
                    } catch (Exception e) {
                        echo "Security audit failed or not available: ${e.getMessage()}"
                    }
                }
            }
        }

        stage('Archive Artifacts') {
            steps {
                // Archive build artifacts
                script {
                    // Find and archive binary files
                    sh 'find target/release -maxdepth 1 -type f -executable -not -name "*.so" -not -name "*.d" | head -10'

                    // Archive the main binary (adjust name as needed)
                    archiveArtifacts artifacts: 'target/release/*', allowEmptyArchive: true, fingerprint: true
                }
            }
        }
    }

    post {
        always {
            // Clean workspace after build
            cleanWs()
        }

        success {
            echo 'Rust build completed successfully!'
        }

        failure {
            echo 'Rust build failed!'
            // Optional: Send notifications
        }

        unstable {
            echo 'Rust build completed with warnings!'
        }
    }
}
