use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

/// Represents output from a PDF operation.
#[derive(Debug, Clone)]
pub enum PdfOutput {
    /// Base64-encoded data URL suitable for returning to MCP clients
    DataUrl {
        data_url: String,
        mime_type: String,
    },
    /// Path to a saved file
    FilePath(String),
    /// Text content (e.g., extracted text, search results)
    Text(String),
    /// Structured JSON data
    Json(serde_json::Value),
}

impl PdfOutput {
    /// Create a data URL from raw bytes and MIME type.
    pub fn data_url(data: Vec<u8>, mime_type: &str) -> Self {
        let encoded = BASE64.encode(&data);
        PdfOutput::DataUrl {
            data_url: format!("data:{mime_type};base64,{encoded}"),
            mime_type: mime_type.to_string(),
        }
    }

    /// Create a PDF data URL from raw bytes.
    pub fn pdf_data_url(data: Vec<u8>) -> Self {
        Self::data_url(data, "application/pdf")
    }

    /// Create a text output.
    pub fn text(text: impl Into<String>) -> Self {
        PdfOutput::Text(text.into())
    }

    /// Create a JSON output from a serializable value.
    pub fn json(value: impl Serialize) -> Self {
        PdfOutput::Json(serde_json::to_value(value).unwrap_or_default())
    }

    /// Convert to the string representation used in MCP tool responses.
    pub fn to_mcp_response(&self) -> String {
        match self {
            PdfOutput::DataUrl { data_url, .. } => data_url.clone(),
            PdfOutput::FilePath(path) => path.clone(),
            PdfOutput::Text(text) => text.clone(),
            PdfOutput::Json(value) => {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
            }
        }
    }
}

/// Format a PDF operation result with a success header.
pub fn format_pdf_response(data: Vec<u8>, message: &str) -> String {
    let data_url = BASE64.encode(&data);
    format!(
        "{message}\n\nResult (base64 encoded PDF): data:application/pdf;base64,{data_url}"
    )
}

/// Format search results as a readable text output.
pub fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No matching PDFs found.".to_string();
    }

    let mut output = format!("Found {} matching PDF(s):\n\n", results.len());
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. {}\n   Size: {} bytes | Modified: {}\n",
            i + 1,
            result.path,
            result.size_bytes,
            result.modified
        ));
    }
    output
}

/// A single search result from filesystem PDF search.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub path: String,
    pub size_bytes: u64,
    pub modified: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_url_output() {
        let output = PdfOutput::data_url(b"hello".to_vec(), "text/plain");
        let response = output.to_mcp_response();
        assert!(response.starts_with("data:text/plain;base64,"));
    }

    #[test]
    fn test_pdf_data_url() {
        let output = PdfOutput::pdf_data_url(b"%PDF-1.4".to_vec());
        let response = output.to_mcp_response();
        assert!(response.starts_with("data:application/pdf;base64,"));
    }

    #[test]
    fn test_text_output() {
        let output = PdfOutput::text("Hello World");
        assert_eq!(output.to_mcp_response(), "Hello World");
    }

    #[test]
    fn test_format_pdf_response() {
        let response = format_pdf_response(b"fake-pdf".to_vec(), "Done");
        assert!(response.contains("Done"));
        assert!(response.contains("base64,"));
    }

    #[test]
    fn test_format_search_results_empty() {
        let results: Vec<SearchResult> = vec![];
        let output = format_search_results(&results);
        assert!(output.contains("No matching"));
    }

    #[test]
    fn test_format_search_results() {
        let results = vec![SearchResult {
            path: "/tmp/test.pdf".to_string(),
            size_bytes: 1024,
            modified: "2024-01-01".to_string(),
        }];
        let output = format_search_results(&results);
        assert!(output.contains("/tmp/test.pdf"));
        assert!(output.contains("1024"));
    }
}
