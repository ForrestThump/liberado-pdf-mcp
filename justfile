# liberado-pdf-mcp development recipes
# Install just: https://github.com/casey/just

DOCKER_TAG := "liberado-pdf-mcp"

# Default target
default:
    @just --list

# Build the Docker image (always builds with GLIBC 2.36 for compat)
build:
    docker build -t {{DOCKER_TAG}} .

# Build with Stirling bridge (adds OCR/watermark/image tools)
build-stirling:
    docker build --build-arg FEATURES="stirling-bridge" -t {{DOCKER_TAG}}:stirling .

# Extract the binary from the Docker image to the given output path
# Usage: just extract-bin [output-path]
extract-bin output="liberado-pdf-mcp":
    docker create --name tmp-liberado {{DOCKER_TAG}}
    docker cp tmp-liberado:/usr/local/bin/liberado-pdf-mcp {{output}}
    docker rm tmp-liberado > /dev/null
    chmod +x {{output}}

# Extract the binary directly into custom-bin for OpenClaw stdio
install-bin:
    docker build -t {{DOCKER_TAG}} .
    @just extract-bin ../../volumes/openclaw/custom-bin/liberado-pdf-mcp

# Check compilation
check:
    cargo check

# Run all tests
test:
    cargo test

# Run tests with Stirling bridge feature
test-stirling:
    cargo test --features stirling-bridge

# Run tests with coverage report
coverage:
    cargo llvm-cov --workspace --summary-only

# Generate HTML coverage report
coverage-html:
    cargo llvm-cov --workspace --html

# Lint all code
lint:
    cargo clippy --all-targets -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Auto-fix lint issues
fix:
    cargo clippy --all-targets --fix --allow-dirty --allow-no-vcs

# Run all checks (test + lint)
ci: test lint
    cargo test --features stirling-bridge
    cargo clippy --all-targets --features stirling-bridge -- -D warnings

# Clean build artifacts
clean:
    cargo clean
