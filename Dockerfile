# Multi-stage Docker build for mcp-pdf-rs
# Produces a ~10–15 MB stripped binary in a distroless image

# ── Stage 1: Build ──
FROM rust:1.94-slim-bookworm AS builder

# cmake is required by aws-lc-sys (pulled in by rustls via turbomcp-http).
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Build with feature flags:
#   Default: native engine + stdio and HTTP transports
#   Stirling: add --features stirling-bridge for OCR/watermark/image tools
#   Minimal: pass --no-default-features --features native,stdio to drop HTTP/TLS deps
ARG FEATURES=""
RUN cargo build --release --bin mcp-pdf-rs ${FEATURES:+--features $FEATURES}

# Strip debug symbols
RUN strip target/release/mcp-pdf-rs

# ── Stage 2: Runtime ──
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /app/target/release/mcp-pdf-rs /usr/local/bin/mcp-pdf-rs

ENV MCP_PDF_ENGINE=native
ENV MCP_PDF_TRANSPORT=stdio

ENTRYPOINT ["/usr/local/bin/mcp-pdf-rs"]
