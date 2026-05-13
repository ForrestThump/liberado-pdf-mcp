use mcp_pdf_server::{PdfServer, ServerConfig};
use mcp_pdf_native::test_utils::minimal_pdf_bytes;
use turbomcp::testing::{McpTestClient, ToolResultAssertions};
use serde_json::json;

fn server() -> PdfServer {
    PdfServer {
        config: ServerConfig::default(),
    }
}

fn base64_pdf() -> String {
    use base64::Engine;
    let data = minimal_pdf_bytes();
    format!(
        "data:application/pdf;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&data)
    )
}

#[tokio::test]
async fn test_ocr_pdf_no_stirling_url() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "ocr_pdf",
            json!({
                "pdf_file": base64_pdf(),
                "languages": "eng"
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

#[tokio::test]
async fn test_add_watermark_no_stirling_url() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "add_watermark",
            json!({
                "pdf_file": base64_pdf(),
                "watermark_text": "CONFIDENTIAL"
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

#[tokio::test]
async fn test_convert_pdf_to_images_no_stirling_url() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "convert_pdf_to_images",
            json!({
                "pdf_file": base64_pdf(),
                "image_format": "png"
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}

#[tokio::test]
async fn test_convert_images_to_pdf_no_stirling_url() {
    let client = McpTestClient::new(server());
    let result = client
        .call_tool(
            "convert_images_to_pdf",
            json!({
                "image_files": [base64_pdf()]
            }),
        )
        .await
        .unwrap();
    result.assert_is_error();
}
