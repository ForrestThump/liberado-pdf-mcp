use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Extract text from all pages of a PDF document.
pub async fn extract_text(input: PdfInput) -> PdfResult<String> {
    let bytes = input.into_bytes().await?;
    let doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let total_pages = doc.get_pages().len() as u32;
    if total_pages == 0 {
        return Ok(String::new());
    }

    let page_numbers: Vec<u32> = (1..=total_pages).collect();
    let text = doc
        .extract_text(&page_numbers)
        .map_err(|e| PdfError::Manipulation(format!("Failed to extract text: {e}")))?;

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::pdf_with_extractable_text;
    use lopdf::Document;

    #[tokio::test]
    async fn test_extract_text_basic() {
        let data = pdf_with_extractable_text("HelloPDF");
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = extract_text(input).await.unwrap();
        assert!(
            result.contains("HelloPDF"),
            "Should extract 'HelloPDF', got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_extract_text_multi_page() {
        use crate::merge::merge_pdfs;

        let data1 = pdf_with_extractable_text("PageOne");
        let data2 = pdf_with_extractable_text("PageTwo");

        let merged = merge_pdfs(
            vec![
                PdfInput::Bytes {
                    data: data1,
                    mime_type: "application/pdf".to_string(),
                    filename: None,
                },
                PdfInput::Bytes {
                    data: data2,
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

        let result = extract_text(input).await.unwrap();
        assert!(
            result.contains("PageOne"),
            "Should contain 'PageOne', got: {result:?}"
        );
        assert!(
            result.contains("PageTwo"),
            "Should contain 'PageTwo', got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_extract_text_empty_pdf() {
        let mut doc = Document::new();
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();

        // Empty pages dictionary (no kids)
        let mut pages = lopdf::Dictionary::new();
        pages.set("Type", lopdf::Object::Name(b"Pages".to_vec()));
        pages.set("Kids", lopdf::Object::Array(vec![]));
        pages.set("Count", lopdf::Object::Integer(0));
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));

        let mut catalog = lopdf::Dictionary::new();
        catalog.set("Type", lopdf::Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", lopdf::Object::Reference(pages_id));
        doc.objects.insert(catalog_id, lopdf::Object::Dictionary(catalog));

        doc.trailer.set("Root", lopdf::Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let input = PdfInput::Bytes {
            data: buf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = extract_text(input).await.unwrap();
        assert_eq!(result, "", "Empty PDF should return empty string");
    }
}
