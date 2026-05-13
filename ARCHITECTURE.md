# ARCHITECTURE.md

## Overview

`mcp-pdf-rs` is an MCP (Model Context Protocol) server that exposes PDF manipulation tools to LLM clients like OpenClaw. It uses a **dual-engine architecture**: a native engine built on [`lopdf`](https://crates.io/crates/lopdf) for core operations (merge, split, extract, rotate, compress, text extraction), and an optional Stirling PDF bridge for advanced features (OCR, watermark, image conversion).

The entire server compiles to a single static binary (~6–10 MB depending on platform). The only external runtime dependency is the optional Stirling PDF server.

## Crate Structure

```
mcp-pdf-rs/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── mcp-pdf-core/          # Shared types, I/O abstraction, error types
│   ├── mcp-pdf-native/        # Native PDF operations (lopdf-based)
│   ├── mcp-pdf-stirling/      # Stirling PDF API bridge (reqwest-based)
│   └── mcp-pdf-server/        # TurboMCP server: tools, config, entry point
```

### Dependency Graph

```
mcp-pdf-server
  ├── turbomcp (MCP framework: macros, transport)
  ├── mcp-pdf-core
  ├── mcp-pdf-native (optional, feature "native")
  └── mcp-pdf-stirling (optional, feature "stirling-bridge")

mcp-pdf-native
  └── mcp-pdf-core

mcp-pdf-stirling
  └── mcp-pdf-core

mcp-pdf-core
  (no sibling crate dependencies)
```

## Crate Details

### `mcp-pdf-core`: Shared Abstractions

**Purpose**: Defines the types and interfaces shared by all other crates. Has zero knowledge of PDF manipulation or networking.

**Modules**:

| Module | File | Purpose |
|---|---|---|
| `error` | `error.rs` | `PdfError` enum (13 variants via `thiserror`), `PdfResult<T>` alias |
| `input` | `input.rs` | `PdfInput` enum — unified input from base64 data URLs, file paths, or raw bytes |
| `output` | `output.rs` | `PdfOutput` enum — unified output as data URLs, text, JSON, or file paths; `SearchResult` struct; formatting helpers |

**Key types**:

- **`PdfInput`** — 2 variants: `FilePath(PathBuf)`, `Bytes { data, mime_type, filename }`. The `from_user_string()` constructor auto-detects the format (data URL, file URI, path, or bare base64) and always resolves to one of these two variants. Methods: `into_bytes()`, `mime_type()`, `filename()`, `is_pdf()`, `is_image()`.

- **`PdfOutput`** — 4 variants: `DataUrl`, `FilePath`, `Text`, `Json`. Method `to_mcp_response()` serializes to the string returned in MCP tool responses.

- **`PdfError`** — Covers input, parse, manipulation, Stirling API, I/O, base64, file-not-found, unsupported format, invalid parameter, and generic errors.

- **`SearchResult`** — `{ path, size_bytes, modified }` used for filesystem search tool output.

### `mcp-pdf-native`: Native PDF Engine

**Purpose**: All PDF operations that can be done locally without external services. Built entirely on `lopdf` (pure-Rust PDF library).

**Dependencies**: `lopdf` (PDF manipulation), `glob` (filesystem globbing), `strsim` (fuzzy string matching).

**Modules**:

| Module | Function | Description |
|---|---|---|
| `merge` | `merge_pdfs(inputs, sort_type)` | Merges multiple PDFs; renumbers objects, rebuilds page tree and catalog |
| `split` | `split_pdf(input, page_numbers)` | Splits at 1-indexed boundaries; uses `delete_pages` internally |
| `extract` | `extract_pages(input, pages)` | Extracts specific pages; validates page range |
| `remove` | `remove_pages(input, pages_to_remove)` | Removes pages; prevents emptying the document |
| `rotate` | `rotate_pdf(input, angle, page_numbers)` | Sets `Rotate` key on page dictionaries; normalizes angles to 0/90/180/270 |
| `compress` | `compress_pdf(input, level)` | Applies `lopdf::Document::compress()` (FlateDecode) |
| `text` | `extract_text(input)` | Uses `lopdf::Document::extract_text()` |
| `info` | `pdf_info(input)` | Reads trailer `/Info` dict; returns JSON with `page_count`, `file_size_bytes`, `version`, metadata, `is_encrypted` |
| `search` | `search_pdfs(base_path, pattern, recursive)` | Filesystem glob for `*.pdf`; filters by filename substring |
| `search` | `fuzzy_match_pdfs(file_list, pattern)` | Normalized Levenshtein distance scoring |
| `search` | `find_related_pdfs(base_path, target_filename, min_occurrences)` | Content-based PDF discovery via word frequency overlap |
| `test_utils` | `minimal_pdf_bytes()`, `pdf_with_extractable_text()` | Test helpers for creating valid PDFs in memory |

**Merge algorithm** (the most complex operation):
1. Load each input PDF as a `lopdf::Document`
2. Optionally sort by filename
3. Renumber objects in each source doc to avoid ID collisions
4. Copy all objects into a new document
5. Collect all page `ObjectId`s from every source
6. Build a new Pages dictionary tree
7. Build a new Catalog pointing to the Pages dict
8. Update trailer Root reference
9. Compress and save

### `mcp-pdf-stirling`: Stirling PDF Bridge

**Purpose**: HTTP client that communicates with a Stirling PDF server for advanced operations not easily implemented in pure Rust.

**Activation**: Only compiled when the `stirling-bridge` feature is enabled. Configured via `STIRLING_PDF_URL` / `STIRLING_PDF_API_KEY` environment variables.

**Dependencies**: `reqwest` (HTTP multipart client).

**Modules**:

| Module | Description |
|---|---|
| `config` | `StirlingConfig` — base URL, API key, timeout, `reqwest::Client`. Constructed from env vars via `from_env()`. |
| `client` | Low-level HTTP helpers: `send_request()` (POST multipart), `send_pdf()` (single file), `send_multiple_files()` (image-to-PDF batch). Handles API key headers and error mapping to `PdfError::StirlingApi`. |
| `ocr` | `ocr_pdf()` — POSTs to `/api/v1/misc/ocr-pdf` with language, deskew, clean params |
| `watermark` | `add_watermark()` — POSTs to `/api/v1/stamp/add-watermark-text` with text, font size, opacity, rotation |
| `convert` | `pdf_to_images()` and `images_to_pdf()` — POSTs to `/api/v1/convert/pdf/img` and `/api/v1/convert/img/pdf` |

**HTTP pattern**: All tools follow the same flow:
1. Resolve `PdfInput` to bytes
2. Build `multipart::Form` with repeated `fileInput` parts (one per file for multi-file endpoints)
3. Add tool-specific form fields
4. POST with `X-API-KEY` header (if configured)
5. Return response bytes on success, `PdfError::StirlingApi` on non-2xx

### `mcp-pdf-server`: MCP Server

**Purpose**: Wires everything together — defines the MCP server, registers tools, handles transport.

**Binary name**: `mcp-pdf-rs` (defined via `[[bin]]` in Cargo.toml).

**Features**:

| Feature | Enables |
|---|---|
| `native` (default) | `mcp-pdf-native` crate |
| `stirling-bridge` | `mcp-pdf-stirling` crate |
| `stdio` (default) | Stdio transport via `turbomcp/stdio` |

**Entry point** (`main.rs`):
1. Initialize `tracing_subscriber` with `RUST_LOG` env filter (default: `info`)
2. Load config from `ServerConfig::from_env()` (reads `MCP_PDF_*` environment variables)
3. Construct `PdfServer` and call `.builder().serve().await`

**Server definition** (`server.rs`):
- `PdfServer` struct holds `ServerConfig`
- `#[server]` macro from turbomcp generates the MCP protocol implementation
- `#[tool]` macros on methods auto-generate JSON schemas from Rust signatures
- Stirling bridge tools delegate to feature-gated `*_impl` functions
- When `stirling-bridge` is not enabled, those functions return a configuration error
- Response formatting uses `format_pdf_response()` (base64 data URL with message) and `PdfOutput`

**Config** (`config.rs`):
- `ServerConfig` struct: `engine` (Native | Stirling), `timeout_seconds`, `transport` (Stdio | Http)
- `from_env()` reads `MCP_PDF_ENGINE`, `MCP_PDF_TRANSPORT`, `MCP_PDF_HTTP_HOST`, `MCP_PDF_HTTP_PORT`
- Stirling connection details (`STIRLING_PDF_URL`, `STIRLING_PDF_API_KEY`, `STIRLING_PDF_TIMEOUT`) are read directly by `StirlingConfig::from_env()` in the stirling crate, not by `ServerConfig`
- Note: TOML config file parsing is planned but not yet implemented; see [Known Gaps](#known-gaps)

## Data Flow

### Native Tool Request (e.g., `merge_pdfs`)

```
MCP Client (e.g. Claude Desktop)
  │  JSON-RPC tools/call { name: "merge_pdfs", arguments: { pdf_files: [...] } }
  ▼
turbomcp stdio transport — deserialize JSON-RPC
  ▼
#[tool] merge_pdfs handler in server.rs
  │  Map each input string → PdfInput::from_user_string()
  ▼
mcp-pdf-native::merge::merge_pdfs(inputs, sort_type)
  │  Resolve PdfInput → bytes → lopdf::Document
  │  Merge documents → Vec<u8>
  ▼
format_pdf_response(bytes, "Successfully merged PDFs")
  │  base64 encode → "data:application/pdf;base64,..."
  ▼
MCP Client receives text response with embedded data URL
```

### Stirling Bridge Tool Request (e.g., `ocr_pdf`)

```
MCP Client
  ▼
ocr_pdf_impl (feature-gated)
  │  Check StirlingConfig::from_env() — error if STIRLING_PDF_URL not set
  │  Resolve PdfInput → bytes
  ▼
mcp-pdf-stirling::ocr::ocr_pdf(config, input, languages, ...)
  │  Build reqwest multipart Form
  │  POST {STIRLING_PDF_URL}/api/v1/misc/ocr-pdf
  ▼
Stirling PDF Server (Java/PDFBox/Tesseract)
  │  Returns processed PDF bytes
  ▼
format_pdf_response(bytes, "OCR completed") → MCP Client
```

## Feature Flag Architecture

The feature flag system is the key to keeping the binary small:

- **`default = ["native", "stdio"]`** — A user who just runs `cargo build` gets native tools only. No `reqwest` (and its ~20 transitive deps) compiled in. Binary is ~6–10 MB depending on platform.
- **`stirling-bridge`** — Adds the Stirling crate and `reqwest`. Binary is larger due to TLS and HTTP deps.
- Tool methods are always registered (the `#[tool]` macro requires them at compile time), but the implementation dispatches to feature-gated functions. When the feature is off, the function returns `McpError::configuration(...)`.

## Testing Strategy

**Unit tests**: Each native module has `#[cfg(test)] mod tests` with in-memory PDF construction. Test utilities (`test_utils.rs`) provide `minimal_pdf_bytes()` and `pdf_with_extractable_text()` to avoid test file fixtures.

**Integration tests**: `crates/mcp-pdf-server/tests/` uses `turbomcp::testing::McpTestClient` to test tools through the full MCP stack — from JSON-RPC call to response assertion, without actually needing a connected MCP transport.

**Stirling integration tests**: `stirling_integration_test.rs` tests that Stirling tools return proper errors when `STIRLING_PDF_URL` is unset — validates the feature-gate fallback paths.

**CI**: GitHub Actions runs `cargo fmt --all --check`, `cargo check` (both feature sets), `cargo test` (both feature sets), `cargo clippy --all-targets -- -D warnings` (both feature sets), and `cargo llvm-cov --workspace` (default features only) with Codecov upload.

## Key Design Decisions

1. **`lopdf` over `pdf` crate**: `lopdf` allows direct object-level PDF manipulation (read, write, modify pages, streams, metadata). The `pdf` crate is primarily read-only/rendering. `lopdf` mirrors PyPDF2's semantics from the Python-based `mcp-pdf-tools`.

2. **Stirling PDF as optional, not mandatory**: The TypeScript-based `mcp-server-stirling-pdf` requires a running Stirling PDF instance for all operations. This server makes the native engine the default for merge/split/extract/rotate/compress/text — operations that work immediately with zero setup.

3. **Base64 data URLs AND file paths**: Claude Desktop provides files as base64 data URLs. Scripts and automations use file paths. Supporting both avoids locking into one client type.

4. **Feature-gated engines**: Users who don't need OCR/watermark shouldn't pay the compile time or binary size cost of `reqwest`.

5. **Filesystem search as a first-class feature**: Content-based PDF search and fuzzy filename matching are unique capabilities from `mcp-pdf-tools` that make the server genuinely useful for document organization.

6. **`PdfInput` abstraction**: All tools accept input through a single enum. The `from_user_string()` constructor auto-detects format (data URL, file URI, raw path, bare base64). This centralizes input parsing and avoids repetition across 15 tool handlers.

## Adding a New Tool

To add a new native tool:

1. **Implement the logic** in `mcp-pdf-native/src/<module>.rs`:
   ```rust
   pub async fn my_tool(input: PdfInput, param: &str) -> PdfResult<Vec<u8>> { ... }
   ```

2. **Add a `#[tool]` method** in `server.rs`:
   ```rust
   #[tool]
   async fn my_tool(&self, pdf_file: String, param: String) -> McpResult<String> {
       let input = PdfInput::from_user_string(&pdf_file)
           .map_err(|e| McpError::invalid_params(e.to_string()))?;
       let result = native::my_module::my_tool(input, &param)
           .await
           .map_err(|e| McpError::tool_execution_failed("my_tool", e.to_string()))?;
       Ok(format_pdf_response(result, "My tool completed"))
   }
   ```

3. **Write unit tests** in the native module and integration tests in `tests/integration_test.rs`.

For a Stirling bridge tool, add the implementation in `mcp-pdf-stirling/` and create a feature-gated `*_impl` function in `server.rs` following the existing pattern.

## Known Gaps

These are items from the original plan (`mcp-pdf-rs-plan.md`) that are not yet implemented:

- **TOML config file**: `ServerConfig::from_env()` reads all configuration from environment variables. The original plan included a TOML config file path but parsing is not implemented.
