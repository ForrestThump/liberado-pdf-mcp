# mcp-pdf-rs development recipes
# Install just: https://github.com/casey/just

# Default target
default:
    @just --list

# Build release binary (native + HTTP, ~10–15 MB)
build:
    cargo build --release --bin mcp-pdf-rs

# Build minimal: native + stdio only, no HTTP/TLS deps (~6–10 MB)
build-minimal:
    cargo build --release --bin mcp-pdf-rs --no-default-features --features native,stdio

# Build with Stirling bridge (adds OCR/watermark/image tools)
build-stirling:
    cargo build --release --bin mcp-pdf-rs --features stirling-bridge

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

# Build Docker image
docker-build:
    docker build -t mcp-pdf-rs .

# Build Docker image with Stirling bridge
docker-build-stirling:
    docker build --build-arg FEATURES="stirling-bridge" -t mcp-pdf-rs:stirling .

# Clean build artifacts
clean:
    cargo clean
