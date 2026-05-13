use mcp_pdf_server::{PdfServer, ServerConfig};
use mcp_pdf_native::test_utils::minimal_pdf_bytes;
use turbomcp::testing::{McpTestClient, ToolResultAssertions};
use serde_json::json;
use std::fs;

struct DirGuard(std::path::PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn server() -> PdfServer {
    PdfServer {
        config: ServerConfig::default(),
    }
}

fn base64_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:application/pdf;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Extract just the base64 data URL from a merge response (which includes a text prefix).
fn extract_data_url(response: &str) -> &str {
    response
        .find("data:")
        .map(|pos| &response[pos..])
        .unwrap_or(response)
}

// ── merge_pdfs ──

#[tokio::test]
async fn test_merge_pdfs_tool() {
    let client = McpTestClient::new(server());
    let pdf1 = base64_data_url(&minimal_pdf_bytes());
    let pdf2 = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("merge_pdfs", json!({"pdf_files": [pdf1, pdf2]}))
        .await
        .unwrap();
    result.assert_text_contains("Successfully merged");
}

#[tokio::test]
async fn test_merge_pdfs_empty_fails() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool("merge_pdfs", json!({"pdf_files": []}))
        .await
        .unwrap();
    result.assert_is_error();
}

// ── split_pdf ──

#[tokio::test]
async fn test_split_pdf_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let merged = client
        .call_tool("merge_pdfs", json!({"pdf_files": [pdf.clone(), pdf]}))
        .await
        .unwrap();
    let merged_text = merged.first_text().unwrap();
    let data_url = extract_data_url(merged_text);
    let result = client
        .call_tool(
            "split_pdf",
            json!({"pdf_file": data_url, "page_numbers": "2"}),
        )
        .await
        .unwrap();
    result.assert_text_contains("Split PDF into");
}

// ── extract_pages ──

#[tokio::test]
async fn test_extract_pages_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("extract_pages", json!({"pdf_file": pdf, "pages": [1]}))
        .await
        .unwrap();
    result.assert_text_contains("Successfully extracted");
}

// ── remove_pages ──

#[tokio::test]
async fn test_remove_pages_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let merged = client
        .call_tool("merge_pdfs", json!({"pdf_files": [pdf.clone(), pdf]}))
        .await
        .unwrap();
    let merged_text = merged.first_text().unwrap();
    let data_url = extract_data_url(merged_text);
    let result = client
        .call_tool(
            "remove_pages",
            json!({"pdf_file": data_url, "pages_to_remove": "1"}),
        )
        .await
        .unwrap();
    result.assert_text_contains("Successfully removed");
}

// ── rotate_pdf ──

#[tokio::test]
async fn test_rotate_pdf_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("rotate_pdf", json!({"pdf_file": pdf, "angle": 90}))
        .await
        .unwrap();
    result.assert_text_contains("Rotated PDF by 90 degrees");
}

// ── compress_pdf ──

#[tokio::test]
async fn test_compress_pdf_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("compress_pdf", json!({"pdf_file": pdf, "level": 1}))
        .await
        .unwrap();
    result.assert_text_contains("Compressed");
}

// ── extract_text ──

#[tokio::test]
async fn test_extract_text_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("extract_text", json!({"pdf_file": pdf}))
        .await
        .unwrap();
    assert!(!result.first_text().unwrap().is_empty());
}

// ── pdf_info ──

#[tokio::test]
async fn test_pdf_info_tool() {
    let client = McpTestClient::new(server());
    let pdf = base64_data_url(&minimal_pdf_bytes());
    let result = client
        .call_tool("pdf_info", json!({"pdf_file": pdf}))
        .await
        .unwrap();
    result.assert_text_contains("Pages:");
    result.assert_text_contains("Size:");
}

#[tokio::test]
async fn test_pdf_info_invalid() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool("pdf_info", json!({"pdf_file": "not-a-valid-pdf"}))
        .await
        .unwrap();
    result.assert_is_error();
}

// ── search_pdfs ──

#[tokio::test]
async fn test_search_pdfs_tool() {
    let client = McpTestClient::new(server());
    // Search the system temp directory with no pattern (should find or return empty)
    let temp = std::env::temp_dir();
    let result = client
        .call_tool(
            "search_pdfs",
            json!({
                "base_path": temp.to_string_lossy(),
                "pattern": "",
                "recursive": false
            }),
        )
        .await
        .unwrap();
    // Should return a response (either results or "No matching PDFs found")
    assert!(!result.first_text().unwrap_or("").is_empty());
}

#[tokio::test]
async fn test_search_pdfs_nonexistent_dir() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "search_pdfs",
            json!({
                "base_path": "Z:\\nonexistent_path_xyz",
                "pattern": "",
                "recursive": false
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

// ── merge_ordered ──

#[tokio::test]
async fn test_merge_ordered_tool() {
    let dir = std::env::temp_dir().join(format!(
        "pdf_test_merge_ordered_{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    let pdf = minimal_pdf_bytes();
    fs::write(dir.join("chapter1.pdf"), &pdf).unwrap();
    fs::write(dir.join("chapter2.pdf"), &pdf).unwrap();

    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "merge_ordered",
            json!({
                "base_path": dir.to_string_lossy(),
                "patterns": ["chapter1", "chapter2"],
                "fuzzy_matching": false
            }),
        )
        .await
        .unwrap();

    result.assert_text_contains("Successfully merged");
}

// ── find_related_pdfs ──

#[tokio::test]
async fn test_find_related_pdfs_tool_no_results() {
    let client = McpTestClient::new(server());
    // Search with a nonexistent target — should error or return empty
    let result = client
        .call_tool(
            "find_related_pdfs",
            json!({
                "base_path": "C:\\",
                "target_filename": "completely_nonexistent_file_xyz.pdf",
                "min_pattern_occurrences": 2
            }),
        )
        .await
        .unwrap();
    // Either an error or empty results is fine
    let text = result.first_text().unwrap_or("");
    assert!(
        !text.is_empty() || result.is_error(),
        "Should have some output"
    );
}

#[tokio::test]
async fn test_find_related_pdfs_tool_default_param() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "find_related_pdfs",
            json!({
                "base_path": "C:\\",
                "target_filename": "nonexistent.pdf"
            }),
        )
        .await
        .unwrap();
    // min_pattern_occurrences defaults to 2 in the tool handler
    let text = result.first_text().unwrap_or("");
    assert!(!text.is_empty() || result.is_error());
}
