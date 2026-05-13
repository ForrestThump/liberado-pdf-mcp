use std::path::PathBuf;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use crate::error::{PdfError, PdfResult};

/// Unified PDF/image input from multiple sources.
#[derive(Debug, Clone)]
pub enum PdfInput {
    /// Local filesystem path
    FilePath(PathBuf),
    /// Raw bytes with explicit MIME type
    Bytes {
        data: Vec<u8>,
        mime_type: String,
        filename: Option<String>,
    },
}

impl PdfInput {
    /// Parse a user-provided string into PdfInput.
    /// Detects: data URLs (base64), file:// URIs, or filesystem paths.
    pub fn from_user_string(s: &str) -> PdfResult<Self> {
        if s.starts_with("data:") {
            Self::from_data_url(s)
        } else if s.starts_with("file://") {
            let path_str = s.strip_prefix("file://").unwrap_or(s);
            // On Windows, file:///C:/... becomes /C:/... — strip leading /
            #[cfg(target_os = "windows")]
            let path_str = path_str.strip_prefix('/').unwrap_or(path_str);
            let path = PathBuf::from(path_str);
            if !path.exists() {
                return Err(PdfError::FileNotFound(path));
            }
            Ok(PdfInput::FilePath(path))
        } else {
            let path = PathBuf::from(s);
            if path.exists() {
                Ok(PdfInput::FilePath(path))
            } else if path.parent().is_some_and(|p| p.exists()) {
                Err(PdfError::FileNotFound(path))
            } else if looks_like_path(&path) {
                // Has path-like characteristics but doesn't exist
                Err(PdfError::FileNotFound(path))
            } else {
                // Try as raw base64
                let decoded = BASE64.decode(s).map_err(|e| {
                    PdfError::Input(format!("Input is not a valid file path, data URL, or base64-encoded PDF: {e}"))
                })?;
                Ok(PdfInput::Bytes { data: decoded, mime_type: "application/pdf".to_string(), filename: None })
            }
        }
    }

    /// Parse a base64 data URL like "data:application/pdf;base64,JVBERi0x..."
    pub fn from_data_url(data_url: &str) -> PdfResult<Self> {
        if !data_url.starts_with("data:") {
            return Err(PdfError::Input("Not a data URL".to_string()));
        }

        // Parse: data:[<mediatype>][;base64],<data>
        let after_prefix = &data_url["data:".len()..];
        let (mime_part, encoded) = after_prefix
            .split_once(',')
            .ok_or_else(|| PdfError::Input("Invalid data URL: no comma separator".to_string()))?;

        let is_base64 = mime_part.ends_with(";base64");
        let mime_type = if is_base64 {
            mime_part.trim_end_matches(";base64")
        } else {
            mime_part
        };

        if mime_type.is_empty() {
            return Err(PdfError::Input("Data URL missing MIME type".to_string()));
        }

        let decoded = if is_base64 {
            BASE64.decode(encoded).map_err(PdfError::Base64)?
        } else {
            // URL-encoded data (rarely used, but support it)
            urlencoding(encoded)?
        };

        Ok(PdfInput::Bytes {
            data: decoded,
            mime_type: mime_type.to_string(),
            filename: None,
        })
    }

    /// Resolve to raw bytes regardless of source.
    pub async fn into_bytes(self) -> PdfResult<Vec<u8>> {
        match self {
            PdfInput::Bytes { data, .. } => Ok(data),
            PdfInput::FilePath(path) => {
                tokio::fs::read(&path).await.map_err(PdfError::Io)
            }
        }
    }

    /// Get the MIME type. For file paths, we guess from extension.
    pub fn mime_type(&self) -> &str {
        match self {
            PdfInput::Bytes { mime_type, .. } => mime_type.as_str(),
            PdfInput::FilePath(path) => {
                // Guess MIME type from extension
                match path.extension().and_then(|e| e.to_str()) {
                    Some("pdf") => "application/pdf",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("gif") => "image/gif",
                    Some("tiff") | Some("tif") => "image/tiff",
                    _ => "application/octet-stream",
                }
            }
        }
    }

    /// Get a display-friendly filename.
    pub fn filename(&self) -> &str {
        match self {
            PdfInput::Bytes { filename, .. } => filename.as_deref().unwrap_or("unnamed.pdf"),
            PdfInput::FilePath(path) => {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed.pdf")
            }
        }
    }

    /// Check if this input appears to be a PDF.
    pub fn is_pdf(&self) -> bool {
        let mime = self.mime_type();
        mime == "application/pdf" || mime == "application/x-pdf"
    }

    /// Returns true if this input likely contains an image.
    pub fn is_image(&self) -> bool {
        let mime = self.mime_type();
        mime.starts_with("image/")
    }
}

/// Simple URL percent-decoding (we avoid another dependency for this tiny function)
fn urlencoding(s: &str) -> PdfResult<Vec<u8>> {
    let mut result = Vec::with_capacity(s.len());
    let mut chars = s.as_bytes().iter().copied();
    while let Some(c) = chars.next() {
        if c == b'%' {
            let hi = chars.next().ok_or_else(|| PdfError::Input("Truncated percent encoding".to_string()))?;
            let lo = chars.next().ok_or_else(|| PdfError::Input("Truncated percent encoding".to_string()))?;
            let byte = hex_to_byte(hi, lo)?;
            result.push(byte);
        } else {
            result.push(c);
        }
    }
    Ok(result)
}

fn hex_to_byte(hi: u8, lo: u8) -> PdfResult<u8> {
    let hi = hex_digit(hi)?;
    let lo = hex_digit(lo)?;
    Ok(hi * 16 + lo)
}

fn hex_digit(b: u8) -> PdfResult<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(PdfError::Input(format!("Invalid hex digit: {}", b as char))),
    }
}

fn looks_like_path(path: &std::path::Path) -> bool {
    // Check for path separators
    let s = path.to_string_lossy();
    s.contains('/') || s.contains('\\') || s.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_data_url_valid() {
        let input = PdfInput::from_data_url(
            "data:application/pdf;base64,SGVsbG8="
        ).unwrap();
        if let PdfInput::Bytes { data, .. } = input {
            assert_eq!(data, b"Hello");
        } else {
            panic!("Expected Bytes variant");
        }
    }

    #[test]
    fn test_from_data_url_not_base64() {
        let input = PdfInput::from_data_url(
            "data:text/plain,Hello%20World"
        ).unwrap();
        if let PdfInput::Bytes { data, .. } = input {
            assert_eq!(data, b"Hello World");
        } else {
            panic!("Expected Bytes variant");
        }
    }

    #[test]
    fn test_from_data_url_invalid() {
        let result = PdfInput::from_data_url("not-a-data-url");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_user_string_data_url() {
        let input = PdfInput::from_user_string(
            "data:application/pdf;base64,SGVsbG8="
        ).unwrap();
        assert_eq!(input.mime_type(), "application/pdf");
    }

    #[test]
    fn test_mime_type_pdf() {
        let input = PdfInput::Bytes {
            data: vec![],
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        assert!(input.is_pdf());
        assert!(!input.is_image());
    }

    #[test]
    fn test_mime_type_image() {
        let input = PdfInput::Bytes {
            data: vec![],
            mime_type: "image/png".to_string(),
            filename: None,
        };
        assert!(input.is_image());
        assert!(!input.is_pdf());
    }

    #[test]
    fn test_filename() {
        let input = PdfInput::Bytes {
            data: vec![],
            mime_type: "application/pdf".to_string(),
            filename: Some("test.pdf".to_string()),
        };
        assert_eq!(input.filename(), "test.pdf");
    }
}
