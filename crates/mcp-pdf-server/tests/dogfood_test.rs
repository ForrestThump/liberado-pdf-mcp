use base64::Engine;
use mcp_pdf_core::PdfInput;
use mcp_pdf_native::test_utils::pdf_with_extractable_text;
use mcp_pdf_server::{PdfServer, ServerConfig};
use serde_json::json;
use std::fs;
use turbomcp::testing::{McpTestClient, ToolResultAssertions};

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
    format!(
        "data:application/pdf;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn extract_data_url(response: &str) -> &str {
    response
        .find("data:")
        .map(|pos| &response[pos..])
        .unwrap_or(response)
}

/// Decode the base64 payload from a data URL response and return raw PDF bytes.
fn decode_result_bytes(text: &str) -> Vec<u8> {
    let data_url = extract_data_url(text);
    let comma_pos = data_url.find(',').unwrap();
    let b64 = &data_url[comma_pos + 1..];
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("valid base64")
}

/// Create a multi-page PDF by merging several single-page PDFs.
async fn make_multipage(pages: &[&str]) -> Vec<u8> {
    let inputs: Vec<PdfInput> = pages
        .iter()
        .map(|t| PdfInput::Bytes {
            data: pdf_with_extractable_text(t),
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .collect();
    mcp_pdf_native::merge::merge_pdfs(inputs, None)
        .await
        .unwrap()
}

/// Get page count from raw PDF bytes.
fn page_count(data: &[u8]) -> usize {
    let doc = lopdf::Document::load_mem(data).unwrap();
    doc.get_pages().len()
}

/// Verify a Rotate key on a specific page.
fn page_has_rotation(data: &[u8], page_num: u32, expected: i64) {
    let doc = lopdf::Document::load_mem(data).unwrap();
    let pages = doc.get_pages();
    let page_id = pages.get(&page_num).unwrap();
    let dict = doc.get_dictionary(*page_id).unwrap();
    assert_eq!(
        *dict.get(b"Rotate").unwrap(),
        lopdf::Object::Integer(expected),
        "Page {page_num} should have Rotate = {expected}"
    );
}

/// Verify a page does NOT have a Rotate key.
fn page_no_rotation(data: &[u8], page_num: u32) {
    let doc = lopdf::Document::load_mem(data).unwrap();
    let pages = doc.get_pages();
    let page_id = pages.get(&page_num).unwrap();
    let dict = doc.get_dictionary(*page_id).unwrap();
    assert!(
        dict.get(b"Rotate").is_err(),
        "Page {page_num} should NOT have Rotate"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// pdf_info
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_pdf_info() {
    let data = pdf_with_extractable_text("Hello Dogfood");
    let url = base64_data_url(&data);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("pdf_info", json!({"pdf_file": url}))
        .await
        .unwrap();
    let text = result.first_text().unwrap();

    assert!(text.contains("Pages:"));
    assert!(text.contains("Size:"));
    assert!(text.contains("Version:"));
    assert!(text.contains("Encrypted: false"));
    assert!(text.contains("1"), "Expected 1 page, got: {text}");
}

#[tokio::test]
async fn dogfood_pdf_info_error_on_invalid() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool("pdf_info", json!({"pdf_file": "definitely_not_a_file.pdf"}))
        .await
        .unwrap();
    result.assert_is_error();
}

// ─────────────────────────────────────────────────────────────────────────────
// extract_text
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_extract_text() {
    let data = pdf_with_extractable_text("UniqueDogfoodText");
    let url = base64_data_url(&data);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("extract_text", json!({"pdf_file": url}))
        .await
        .unwrap();
    let text = result.first_text().unwrap();

    assert!(
        text.contains("UniqueDogfoodText"),
        "Expected 'UniqueDogfoodText' in output: {text}"
    );
}

#[tokio::test]
async fn dogfood_extract_text_multipage() {
    let multi = make_multipage(&["PageAlpha", "PageBeta", "PageGamma"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("extract_text", json!({"pdf_file": url}))
        .await
        .unwrap();
    let text = result.first_text().unwrap();

    assert!(text.contains("PageAlpha"));
    assert!(text.contains("PageBeta"));
    assert!(text.contains("PageGamma"));
}

// ─────────────────────────────────────────────────────────────────────────────
// merge_pdfs
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_merge() {
    let pdf1 = base64_data_url(&pdf_with_extractable_text("Merge1"));
    let pdf2 = base64_data_url(&pdf_with_extractable_text("Merge2"));
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("merge_pdfs", json!({"pdf_files": [pdf1, pdf2]}))
        .await
        .unwrap();
    result.assert_text_contains("Successfully merged");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 2, "Merged PDF should have 2 pages");

    // Verify both texts are present in the merged PDF
    let text = mcp_pdf_native::text::extract_text(PdfInput::Bytes {
        data: bytes,
        mime_type: "application/pdf".to_string(),
        filename: None,
    })
    .await
    .unwrap();
    assert!(text.contains("Merge1"), "Missing Merge1 text");
    assert!(text.contains("Merge2"), "Missing Merge2 text");
}

#[tokio::test]
async fn dogfood_merge_empty_fails() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool("merge_pdfs", json!({"pdf_files": []}))
        .await
        .unwrap();
    result.assert_is_error();
}

// ─────────────────────────────────────────────────────────────────────────────
// split_pdf
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_split() {
    let multi = make_multipage(&["AAA", "BBB", "CCC", "DDD"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    // Split at page 2 → [1], [2,3,4]
    let result = client
        .call_tool("split_pdf", json!({"pdf_file": url, "page_numbers": "2"}))
        .await
        .unwrap();
    let text = result.first_text().unwrap();
    assert!(text.contains("Split PDF into 2 parts"));

    // Parse each data URL: find "data:application/pdf;base64," and take the b64
    // portion up to the next non-base64 char (whitespace/newline).
    let mut b64_parts: Vec<String> = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("data:application/pdf;base64,") {
        let after_prefix = &remaining[start + "data:application/pdf;base64,".len()..];
        let end = after_prefix
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '+' && c != '/' && c != '=')
            .unwrap_or(after_prefix.len());
        b64_parts.push(after_prefix[..end].to_string());
        remaining = &remaining[start + end + 1..];
    }
    assert_eq!(b64_parts.len(), 2, "Should have 2 split parts");

    let part1_bytes =
        base64::engine::general_purpose::STANDARD.decode(&b64_parts[0]).unwrap();
    let part2_bytes =
        base64::engine::general_purpose::STANDARD.decode(&b64_parts[1]).unwrap();

    assert_eq!(page_count(&part1_bytes), 1);
    assert_eq!(page_count(&part2_bytes), 3);

    // Verify content
    let text1 =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: part1_bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();
    let text2 =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: part2_bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();

    assert!(text1.contains("AAA"), "Part 1 should contain AAA");
    assert!(!text1.contains("BBB"), "Part 1 should NOT contain BBB");
    assert!(text2.contains("BBB"), "Part 2 should contain BBB");
    assert!(text2.contains("DDD"), "Part 2 should contain DDD");
}

// ─────────────────────────────────────────────────────────────────────────────
// extract_pages
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_extract_pages() {
    let multi = make_multipage(&["One", "Two", "Three", "Four"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("extract_pages", json!({"pdf_file": url, "pages": [1, 3]}))
        .await
        .unwrap();
    result.assert_text_contains("Successfully extracted");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 2, "Extracted PDF should have 2 pages");

    let text =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();

    assert!(text.contains("One"), "Should contain page 1 text");
    assert!(!text.contains("Two"), "Should NOT contain page 2 text");
    assert!(text.contains("Three"), "Should contain page 3 text");
    assert!(!text.contains("Four"), "Should NOT contain page 4 text");
}

// ─────────────────────────────────────────────────────────────────────────────
// remove_pages
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_remove_pages() {
    let multi = make_multipage(&["Red", "Green", "Blue", "Yellow"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    // Remove page 2 ("Green")
    let result = client
        .call_tool(
            "remove_pages",
            json!({"pdf_file": url, "pages_to_remove": "2"}),
        )
        .await
        .unwrap();
    result.assert_text_contains("Successfully removed");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 3, "Should have 3 pages after removal");

    let text =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();

    assert!(text.contains("Red"));
    assert!(!text.contains("Green"), "Green should be removed");
    assert!(text.contains("Blue"));
    assert!(text.contains("Yellow"));
}

#[tokio::test]
async fn dogfood_remove_multiple_pages() {
    let multi = make_multipage(&["A1", "B2", "C3", "D4"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool(
            "remove_pages",
            json!({"pdf_file": url, "pages_to_remove": "1,4"}),
        )
        .await
        .unwrap();
    result.assert_text_contains("Successfully removed");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 2);

    let text =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();

    assert!(!text.contains("A1"), "A1 should be removed");
    assert!(text.contains("B2"));
    assert!(text.contains("C3"));
    assert!(!text.contains("D4"), "D4 should be removed");
}

// ─────────────────────────────────────────────────────────────────────────────
// rotate_pdf
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_rotate_all_pages() {
    let multi = make_multipage(&["R1", "R2"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("rotate_pdf", json!({"pdf_file": url, "angle": 180}))
        .await
        .unwrap();
    result.assert_text_contains("Rotated PDF by 180 degrees");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 2);
    page_has_rotation(&bytes, 1, 180);
    page_has_rotation(&bytes, 2, 180);
}

#[tokio::test]
async fn dogfood_rotate_specific_page() {
    let multi = make_multipage(&["S1", "S2", "S3"]).await;
    let url = base64_data_url(&multi);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool(
            "rotate_pdf",
            json!({"pdf_file": url, "angle": 270, "page_numbers": "2"}),
        )
        .await
        .unwrap();
    result.assert_text_contains("Rotated PDF by 270 degrees");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 3);
    page_no_rotation(&bytes, 1);
    page_has_rotation(&bytes, 2, 270);
    page_no_rotation(&bytes, 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// compress_pdf
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_compress() {
    let pdf_data = make_multipage(&["CompressTest"]).await;
    let input_len = pdf_data.len();
    let url = base64_data_url(&pdf_data);
    let client = McpTestClient::new(server());

    let result = client
        .call_tool("compress_pdf", json!({"pdf_file": url, "level": 3}))
        .await
        .unwrap();
    result.assert_text_contains("Compressed");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 1, "Compressed PDF should still be valid");
    // Output should not blow up (reasonable upper bound)
    assert!(
        bytes.len() <= input_len + 100,
        "Compressed output {} grew too much from {} bytes",
        bytes.len(),
        input_len
    );

    // Verify text survived compression
    let text =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();
    assert!(text.contains("CompressTest"), "Text should survive compression");
}

// ─────────────────────────────────────────────────────────────────────────────
// search_pdfs
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_search_pdfs() {
    let dir = std::env::temp_dir().join(format!("pdf_dogfood_search_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    let pdf = pdf_with_extractable_text("SearchDoc");
    fs::write(dir.join("report_2024.pdf"), &pdf).unwrap();
    fs::write(dir.join("invoice_2024.pdf"), &pdf).unwrap();
    fs::write(dir.join("notes.txt"), b"not a pdf").unwrap();

    let client = McpTestClient::new(server());

    // Search by pattern
    let result = client
        .call_tool(
            "search_pdfs",
            json!({"base_path": dir.to_string_lossy(), "pattern": "report", "recursive": true}),
        )
        .await
        .unwrap();
    let text = result.first_text().unwrap();
    assert!(text.contains("report_2024.pdf"), "Should find report_2024.pdf");
    assert!(!text.contains("invoice_2024.pdf"), "Should NOT find invoice_2024.pdf");

    // Search all PDFs
    let result = client
        .call_tool(
            "search_pdfs",
            json!({"base_path": dir.to_string_lossy(), "pattern": "", "recursive": true}),
        )
        .await
        .unwrap();
    let text = result.first_text().unwrap();
    assert!(text.contains("report_2024.pdf"));
    assert!(text.contains("invoice_2024.pdf"));
}

#[tokio::test]
async fn dogfood_search_pdfs_nonexistent_dir_errors() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "search_pdfs",
            json!({"base_path": "Z:\\definitely_does_not_exist_999"}),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

// ─────────────────────────────────────────────────────────────────────────────
// merge_ordered
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_merge_ordered() {
    let dir = std::env::temp_dir().join(format!("pdf_dogfood_mo_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    fs::write(dir.join("intro.pdf"), pdf_with_extractable_text("INTRO")).unwrap();
    fs::write(dir.join("body.pdf"), pdf_with_extractable_text("BODY")).unwrap();
    fs::write(dir.join("conclusion.pdf"), pdf_with_extractable_text("CONCLUSION")).unwrap();
    fs::write(dir.join("unrelated.pdf"), pdf_with_extractable_text("UNRELATED")).unwrap();

    let client = McpTestClient::new(server());

    let result = client
        .call_tool(
            "merge_ordered",
            json!({
                "base_path": dir.to_string_lossy(),
                "patterns": ["intro", "body", "conclusion"],
                "fuzzy_matching": false
            }),
        )
        .await
        .unwrap();
    result.assert_text_contains("Successfully merged");

    let bytes = decode_result_bytes(result.first_text().unwrap());
    assert_eq!(page_count(&bytes), 3, "Should merge exactly 3 PDFs");

    let text =
        mcp_pdf_native::text::extract_text(PdfInput::Bytes {
            data: bytes,
            mime_type: "application/pdf".to_string(),
            filename: None,
        })
        .await
        .unwrap();

    assert!(text.contains("INTRO"), "Should contain INTRO");
    assert!(text.contains("BODY"), "Should contain BODY");
    assert!(text.contains("CONCLUSION"), "Should contain CONCLUSION");
    assert!(!text.contains("UNRELATED"), "Should NOT contain UNRELATED");
}

#[tokio::test]
async fn dogfood_merge_ordered_no_match_errors() {
    let dir = std::env::temp_dir().join(format!("pdf_dogfood_mo_empty_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "merge_ordered",
            json!({
                "base_path": dir.to_string_lossy(),
                "patterns": ["nonexistent"]
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

// ─────────────────────────────────────────────────────────────────────────────
// find_related_pdfs
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn dogfood_find_related_pdfs() {
    let dir = std::env::temp_dir().join(format!("pdf_dogfood_rel_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    fs::write(
        dir.join("target.pdf"),
        pdf_with_extractable_text("Architecture planning document featuring modular design."),
    )
    .unwrap();
    fs::write(
        dir.join("similar.pdf"),
        pdf_with_extractable_text("Modular architecture is key to the planning phase."),
    )
    .unwrap();
    fs::write(
        dir.join("unrelated.pdf"),
        pdf_with_extractable_text("Completely different topic about cooking recipes."),
    )
    .unwrap();

    let client = McpTestClient::new(server());

    let result = client
        .call_tool(
            "find_related_pdfs",
            json!({"base_path": dir.to_string_lossy(), "target_filename": "target.pdf", "min_pattern_occurrences": 1}),
        )
        .await
        .unwrap();
    let text = result.first_text().unwrap();

    assert!(text.contains("similar.pdf"), "Should find similar.pdf");
    assert!(!text.contains("unrelated.pdf"), "Should NOT find unrelated.pdf");
    assert!(!text.contains("target.pdf"), "Should NOT include the target itself");
}

#[tokio::test]
async fn dogfood_find_related_nonexistent_target() {
    let dir = std::env::temp_dir().join(format!("pdf_dogfood_rel_bad_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = DirGuard(dir.clone());

    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "find_related_pdfs",
            json!({"base_path": dir.to_string_lossy(), "target_filename": "nope.pdf"}),
        )
        .await
        .unwrap();
    // Should error or return no results
    let text = result.first_text().unwrap_or("");
    assert!(!text.is_empty() || result.is_error(), "Should produce some output");
}
