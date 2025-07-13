pipeline {
    agent any

    triggers {
        // Trigger build on SCM changes (pushes)
        scm('H/5 * * * *')  // Poll SCM every 5 minutes
    }

    environment {
        // Rust environment variables
        CARGO_HOME = '/root/.cargo'
        RUSTUP_HOME = '/root/.rustup'
        PATH = "${env.PATH}:/root/.cargo/bin"
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

        stage('Setup Rust') {
            steps {
                script {
                    // Check if rustup is installed, install if not
                    def rustupExists = sh(script: 'test -f /root/.cargo/bin/rustup', returnStatus: true) == 0
                    if (!rustupExists) {
                        echo 'Installing rustup...'
                        sh 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
                    }

                    // Update rustup and install stable toolchain
                    sh '/root/.cargo/bin/rustup update stable'
                    sh '/root/.cargo/bin/rustup default stable'

                    // Install additional components
                    sh '/root/.cargo/bin/rustup component add clippy rustfmt'
                }
            }
        }

        stage('Rust Info') {
            steps {
                // Display Rust and Cargo versions
                sh '/root/.cargo/bin/rustc --version'
                sh '/root/.cargo/bin/cargo --version'
            }
        }

        stage('Cache Dependencies') {
            steps {
                // Update Cargo index and cache dependencies
                sh '/root/.cargo/bin/cargo fetch'
            }
        }

        stage('Lint') {
            steps {
                // Run clippy for linting
                sh '/root/.cargo/bin/cargo clippy -- -D warnings'
            }
        }

        stage('Format Check') {
            steps {
                // Check code formatting
                sh '/root/.cargo/bin/cargo fmt -- --check'
            }
        }

        stage('Build') {
            steps {
                // Build the project
                sh '/root/.cargo/bin/cargo build --release'
            }
        }

        stage('Test') {
            steps {
                // Run tests
                sh '/root/.cargo/bin/cargo test --release'
            }
        }

        stage('Security Audit') {
            steps {
                // Optional: Run security audit
                script {
                    t
