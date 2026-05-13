# liberado-pdf-mcp

A Rust-based [Model Context Protocol (MCP)](https://modelcontextprotocol.io) server for PDF manipulation. Built with [TurboMCP](https://github.com/Epistates/turbomcp) for OpenClaw and other compatible MCP clients.

## Features

- **15 tools** for PDF manipulation: merge, split, extract, remove, rotate, compress, text extraction, metadata, filesystem search, fuzzy matching, content-based related PDF discovery, OCR, watermark, PDF-to-images, and images-to-PDF
- **Dual-engine architecture**: native PDF operations via [`lopdf`](https://crates.io/crates/lopdf) with zero external dependencies, plus an optional Stirling PDF bridge for OCR, watermark, and image conversion
- **Small binary**: ~10 MB executable (native + HTTP build, platform-dependent), no runtime dependencies
- **Flexible input**: accepts base64 data URLs, `file://` URIs, or local filesystem paths
- **Feature-gated**: build only what you need; the Stirling bridge is optional
- **Docker support**: multi-stage build producing a distroless container

## Quick Start

### Install via Cargo

```bash
cargo install --git https://github.com/ForrestThump/liberado-pdf-mcp
```

### Install via Docker

```bash
docker build -t liberado-pdf-mcp .
```
(~50MB disk usage)

With Stirling bridge enabled:

```bash
docker build --build-arg FEATURES="stirling-bridge" -t liberado-pdf-mcp:stirling .
```

### Claude Desktop Configuration

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "mcp-pdf-rs": {
      "command": "mcp-pdf-rs"
    }
  }
}
```

For the Stirling bridge variant, first install the binary built with `--features stirling-bridge`, then point it at your Stirling PDF server via environment:

```json
{
  "mcpServers": {
    "mcp-pdf-rs": {
      "command": "mcp-pdf-rs",
      "env": {
        "STIRLING_PDF_URL": "http://localhost:8080"
      }
    }
  }
}
```

## Configuration

Configuration is layered (later sources override earlier):

1. Compile-time defaults
2. Environment variables (`MCP_PDF_*`)

| Environment Variable | Description | Default |
|---|---|---|
| `MCP_PDF_ENGINE` | Engine: `native` or `stirling` | `native` |
| `MCP_PDF_TRANSPORT` | Transport: `stdio` or `http` | `stdio` |
| `MCP_PDF_HTTP_HOST` | HTTP bind address (when transport=http) | `0.0.0.0` |
| `MCP_PDF_HTTP_PORT` | HTTP bind port (when transport=http) | `8080` |
| `STIRLING_PDF_URL` | Stirling PDF server URL | — (required for Stirling tools) |
| `STIRLING_PDF_API_KEY` | API key for Stirling PDF security | — |
| `STIRLING_PDF_TIMEOUT` | Stirling request timeout in seconds | `120` |

## Tools

### Native Engine (always available)

| Tool | Description |
|---|---|
| `merge_pdfs` | Merge multiple PDFs into one; supports alphabetical/reverse sort |
| `split_pdf` | Split a PDF at specified page numbers |
| `extract_pages` | Extract specific pages to a new PDF |
| `remove_pages` | Remove specified pages from a PDF |
| `rotate_pdf` | Rotate pages by 0, 90, 180, or 270 degrees |
| `compress_pdf` | Compress PDF stream data to reduce file size |
| `extract_text` | Extract plain text from all pages |
| `pdf_info` | Get page count, file size, version, metadata, encryption status |
| `search_pdfs` | Search filesystem for PDFs by filename pattern (supports recursive glob) |
| `merge_ordered` | Merge PDFs in a specific order using filename patterns (supports fuzzy matching) |
| `find_related_pdfs` | Find PDFs related by text content (word frequency analysis) |

### Stirling Bridge (requires `STIRLING_PDF_URL`)

| Tool | Description |
|---|---|
| `ocr_pdf` | Perform OCR on a scanned PDF (supports multiple languages) |
| `add_watermark` | Add a text watermark with configurable opacity and rotation |
| `convert_pdf_to_images` | Convert PDF pages to PNG, JPG, or GIF |
| `convert_images_to_pdf` | Convert images to a PDF document |

## Input Formats

All tools accept PDFs in any of these formats:

- **Base64 data URL**: `data:application/pdf;base64,JVBERi0x...`
- **File URI**: `file:///home/user/document.pdf`
- **Filesystem path**: `/home/user/document.pdf`

The server auto-detects the format.

## Features

| Feature | Description |
|---|---|
| `native` (default) | Native engine using `lopdf` |
| `stirling-bridge` | Optional Stirling PDF bridge for OCR, watermark, image conversion |
| `stdio` (default) | Stdio transport for Claude Desktop |
| `http` (default) | HTTP/Streamable-HTTP transport via axum |

Build combinations:

```bash
# Default: native engine, stdio + HTTP transports
cargo build --release

# Minimal: stdio only (no HTTP/axum deps, smaller binary)
cargo build --release --no-default-features --features native,stdio

# With Stirling bridge
cargo build --release --features stirling-bridge
```

## Development

```bash
# Build
just build

# Run tests
just test

# Lint
just lint

# Auto-fix lint issues
just fix

# Run all CI checks
just ci

# Coverage report
just coverage
```

### Prerequisites

- Rust 1.89+ (edition 2024)
- [just](https://github.com/casey/just) (optional, for recipe shortcuts)

## License

MIT
