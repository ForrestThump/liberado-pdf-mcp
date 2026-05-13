use std::collections::HashSet;
use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Extract specific 1-indexed pages from a PDF and return as a new PDF.
///
/// Only the pages listed in `pages` will be included in the output.
pub async fn extract_pages(input: PdfInput, pages: &[u32]) -> PdfResult<Vec<u8>> {
    if pages.is_empty() {
        return Err(PdfError::InvalidParameter(
            "No pages specified to extract".to_string(),
        ));
    }
    let bytes = input.into_bytes().await?;
    let doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidParameter(
            "PDF has no pages".to_string(),
        ));
    }

    for &p in pages {
        if p < 1 || p > total {
            return Err(PdfError::InvalidParameter(format!(
                "Page {p} out of range (1-{total})"
            )));
        }
    }

    let keep_set: HashSet<u32> = pages.iter().copied().collect();
    // delete_pages expects 1-indexed page numbers
    let to_remove: Vec<u32> = (1..=total)
        .filter(|p| !keep_set.contains(p))
        .collect();

    let mut new_doc = doc;
    if !to_remove.is_empty() {
        new_doc.delete_pages(&to_remove);
    }

    let mut buf = Vec::new();
    new_doc
        .save_to(&mut buf)
        .map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::merge_pdfs;
    use crate::test_utils::minimal_pdf_bytes;

    #[tokio::test]
    async fn test_extract_pages() {
        let pdf = minimal_pdf_bytes();

        // Merge 3 copies to get a 3-page PDF
        let merged = merge_pdfs(
                vec![
                    PdfInput::Bytes {
                        data: pdf.clone(),
                        mime_type: "application/pdf".to_string(),
                        filename: None,
                    },
                    PdfInput::Bytes {
                        data: pdf.clone(),
                        mime_type: "application/pdf".to_string(),
                        filename: None,
                    },
                    PdfInput::Bytes {
                        data: pdf,
                        mime_type: "application/pdf".to_string(),
                        filename: None,
                    },
                ],
                None,
            )
            .await
            .unwrap();

        let input = PdfInput::Bytes {
            data: merged,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let result = extract_pages(input, &[1, 3]).await.unwrap();
        let extracted = Document::load_mem(&result).unwrap();
        assert_eq!(extracted.get_pages().len(), 2);
    }

    #[tokio::test]
    async fn test_extract_single_page() {
        let pdf = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data: pdf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let result = extract_pages(input, &[1]).await.unwrap();
        let extracted = Document::load_mem(&result).unwrap();
        assert_eq!(extracted.get_pages().len(), 1);
    }

    #[tokio::test]
    async fn test_extract_invalid_page() {
        let pdf = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data: pdf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let result = extract_pages(input, &[99]).await;
        assert!(result.is_err());
    }
}
