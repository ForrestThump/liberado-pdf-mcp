use std::path::PathBuf;
use thiserror::Error;

pub type PdfResult<T> = Result<T, PdfError>;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("Input error: {0}")]
    Input(String),

    #[error("PDF parsing failed: {0}")]
    Parse(String),

    #[error("PDF manipulation failed: {0}")]
    Manipulation(String),

    #[error("Stirling PDF API error (status {status}): {message}")]
    StirlingApi { status: u16, message: String },

    #[error("Stirling PDF not configured. Set STIRLING_PDF_URL.")]
    StirlingNotConfigured,

    #[error("Tool not available: {tool}. Reason: {reason}")]
    ToolUnavailable { tool: String, reason: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Not a PDF file: {0}")]
    NotPdf(String),

    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_error_display() {
        assert_eq!(
            PdfError::Input("bad input".into()).to_string(),
            "Input error: bad input"
        );
        assert_eq!(
            PdfError::Parse("corrupt pdf".into()).to_string(),
            "PDF parsing failed: corrupt pdf"
        );
        assert_eq!(
            PdfError::Manipulation("failed".into()).to_string(),
            "PDF manipulation failed: failed"
        );
        assert_eq!(
            PdfError::StirlingNotConfigured.to_string(),
            "Stirling PDF not configured. Set STIRLING_PDF_URL."
        );
        assert_eq!(
            PdfError::StirlingApi {
                status: 500,
                message: "boom".into()
            }
            .to_string(),
            "Stirling PDF API error (status 500): boom"
        );
        assert_eq!(
            PdfError::ToolUnavailable {
                tool: "ocr".into(),
                reason: "disabled".into()
            }
            .to_string(),
            "Tool not available: ocr. Reason: disabled"
        );
        assert_eq!(
            PdfError::InvalidParameter("bad".into()).to_string(),
            "Invalid parameter: bad"
        );
        assert_eq!(
            PdfError::NotPdf("bad".into()).to_string(),
            "Not a PDF file: bad"
        );
        assert_eq!(
            PdfError::UnsupportedFormat("tiff".into()).to_string(),
            "Unsupported format: tiff"
        );
        assert_eq!(PdfError::Other("misc".into()).to_string(), "misc");
    }

    #[test]
    fn test_pdf_error_file_not_found() {
        let err = PdfError::FileNotFound(std::path::PathBuf::from("/nonexistent.pdf"));
        assert!(err.to_string().contains("/nonexistent.pdf"));
        assert!(err.to_string().contains("File not found"));
    }

    #[test]
    fn test_pdf_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let pdf_err: PdfError = io_err.into();
        assert!(pdf_err.to_string().contains("file not found"));
    }

    #[test]
    fn test_pdf_error_from_base64() {
        use base64::Engine;
        let result = base64::engine::general_purpose::STANDARD.decode("!!!invalid!!!");
        let pdf_err: PdfError = result.unwrap_err().into();
        assert!(pdf_err.to_string().contains("Base64 decode error"));
    }

    #[test]
    fn test_pdf_result_ok() {
        let result: PdfResult<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.as_ref().ok(), Some(&42));
    }

    #[test]
    fn test_pdf_result_err() {
        let result: PdfResult<i32> = Err(PdfError::Other("fail".into()));
        assert!(result.is_err());
    }
}
