# Multi-stage Docker build for liberado-pdf-mcp
# Built inside rust:1.94-slim-bookworm (Debian 12 / GLIBC 2.36) for
# compatibility with Debian 12-based runtimes like the OpenClaw container.
# Always build via Docker (not host cargo) to avoid GLIBC version skew.

ARG TESSDATA_PREFIX=/usr/share/tessdata

# ── Stage 1: Build ──
FROM rust:1.94-slim-bookworm AS builder

ARG TESSDATA_PREFIX

# cmake / g++ / make are required by aws-lc-sys (pulled in by rustls via turbomcp-http)
# and by kreuzberg-tesseract (compiles Tesseract + Leptonica from source for static linking).
# wget is needed to download Tesseract language data.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    cmake \
    make \
    g++ \
    wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Build with feature flags:
#   Default: native engine + native OCR + stdio and HTTP transports
#   Stirling: add stirling-bridge for legacy Stirling PDF OCR/watermark/image tools
#   Minimal: pass --no-default-features --features native,stdio to drop HTTP/TLS deps
ARG FEATURES="native-ocr"
RUN cargo build --release --bin liberado-pdf-mcp ${FEATURES:+--features $FEATURES}

# Strip debug symbols
RUN strip target/release/liberado-pdf-mcp

# Download Tesseract trained language data for native OCR
RUN mkdir -p ${TESSDATA_PREFIX} && \
    wget -q -O ${TESSDATA_PREFIX}/eng.traineddata \
        https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata

# ── Stage 2: Binary export (for OpenClaw stdio / custom-bin) ──
# Build via: docker build --target export -t liberado-pdf-mcp-export .
# Extract via:
#   docker create --name tmp --entrypoint "" liberado-pdf-mcp-export
#   docker cp tmp:/liberado-pdf-mcp ./liberado-pdf-mcp
#   docker rm tmp
FROM scratch AS export
COPY --from=builder /app/target/release/liberado-pdf-mcp /liberado-pdf-mcp

# ── Stage 3: Runtime (default target) ──
FROM gcr.io/distroless/cc-debian12:nonroot

ARG TESSDATA_PREFIX

COPY --from=builder /app/target/release/liberado-pdf-mcp /usr/local/bin/liberado-pdf-mcp
COPY --from=builder ${TESSDATA_PREFIX}/eng.traineddata ${TESSDATA_PREFIX}/eng.traineddata

ENV MCP_PDF_ENGINE=native
ENV MCP_PDF_TRANSPORT=http
ENV TESSDATA_PREFIX=${TESSDATA_PREFIX}

ENTRYPOINT ["/usr/local/bin/liberado-pdf-mcp"]
