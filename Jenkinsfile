pipeline {
    agent {
        dockerContainer {
            image 'rust:1.75'
        }
    }

    environment {
        CARGO_HOME = '/usr/local/cargo'
        RUSTUP_HOME = '/usr/local/rustup'
    }

    stages {
        stage('Checkout') {
            steps {
                checkout scm
            }
        }

        stage('Setup Environment') {
            steps {
                sh 'cargo --version'
                sh 'rustc --version'
                sh 'mkdir -p target'
                sh 'mkdir -p ~/.cargo/registry'
            }
        }

        stage('Build Dependencies') {
            steps {
                sh 'cargo fetch'
            }
        }

        stage('Run Tests') {
            steps {
                sh 'cargo test --release'
            }
        }

        stage('Build Binary') {
            steps {
                sh 'cargo build --release'
            }
        }

        stage('Archive') {
            steps {
                archiveArtifacts artifacts: 'target/release/**', allowEmptyArchive: true
            }
        }
    }

    post {
        always {
            cleanWs()
        }
        success {
            echo 'Rust build completed successfully!'
        }
        failure {
            echo 'Rust build failed!'
        }
    }
}
