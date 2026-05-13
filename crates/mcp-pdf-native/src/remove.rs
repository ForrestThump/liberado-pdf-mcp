use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Remove specified pages from a PDF document.
///
/// `pages_to_remove` is a comma/semicolon-separated string of
/// 1-indexed page numbers to remove.
/// The document must retain at least one page.
pub async fn remove_pages(input: PdfInput, pages_to_remove: &str) -> PdfResult<Vec<u8>> {
    let bytes = input.into_bytes().await?;
    let mut doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidParameter(
            "PDF has no pages".to_string(),
        ));
    }

    // Parse 1-indexed page numbers, rejecting any invalid input
    let mut remove_set: Vec<u32> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for token in pages_to_remove.split([',', ';']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        match token.parse::<u32>() {
            Ok(n) if n >= 1 && n <= total => remove_set.push(n),
            Ok(n) => errors.push(format!("Page {n} out of range (1-{total})")),
            Err(_) => errors.push(format!("'{token}' is not a valid page number")),
        }
    }

    if !errors.is_empty() {
        return Err(PdfError::InvalidParameter(format!(
            "Invalid page numbers: {}",
            errors.join("; ")
        )));
    }

    if remove_set.is_empty() {
        return Err(PdfError::InvalidParameter(
            "No page numbers provided to remove".to_string(),
        ));
    }

    // Deduplicate to avoid underflow when same page is listed multiple times
    remove_set.sort_unstable();
    remove_set.dedup();

    // delete_pages expects 1-indexed page numbers
    let remaining = total - remove_set.len() as u32;
    if remaining == 0 {
        return Err(PdfError::InvalidParameter(
            "Cannot remove all pages — document would be empty".to_string(),
        ));
    }

    doc.delete_pages(&remove_set);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::merge_pdfs;
    use crate::test_utils::minimal_pdf_bytes;

    #[tokio::test]
    async fn test_remove_pages() {
        let pdf = minimal_pdf_bytes();

        // Create a 3-page PDF by merging
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
        let result = remove_pages(input, "2").await.unwrap();
        let doc = Document::load_mem(&result).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[tokio::test]
    async fn test_remove_multiple_pages() {
        let pdf = minimal_pdf_bytes();

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
        let result = remove_pages(input, "1,3").await.unwrap();
        let doc = Document::load_mem(&result).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[tokio::test]
    async fn test_remove_all_pages_fails() {
        let pdf = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data: pdf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let result = remove_pages(input, "1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_invalid_page() {
        let pdf = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data: pdf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let result = remove_pages(input, "99").await;
        assert!(result.is_err());
    }
}
