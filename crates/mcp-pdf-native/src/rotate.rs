use lopdf::{Document, Object};
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Rotate pages in a PDF document.
///
/// `angle` should be one of 0, 90, 180, 270 (will be normalized).
/// `page_numbers` is an optional comma/semicolon-separated list of
/// 1-indexed page numbers to rotate. If `None`, all pages are rotated.
pub async fn rotate_pdf(
    input: PdfInput,
    angle: i32,
    page_numbers: Option<&str>,
) -> PdfResult<Vec<u8>> {
    let bytes = input.into_bytes().await?;
    let mut doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    // Normalize angle to 0, 90, 180, 270
    let normalized = ((angle % 360) + 360) % 360;
    if ![0, 90, 180, 270].contains(&normalized) {
        return Err(PdfError::InvalidParameter(format!(
            "Invalid rotation angle: {angle}. Must resolve to 0, 90, 180, or 270 (mod 360). Got {normalized}."
        )));
    }

    let total_pages = doc.get_pages().len() as u32;
    let pages_to_rotate: Vec<u32> = match page_numbers {
        None | Some("") => (1..=total_pages).collect(),
        Some(s) => s
            .split([',', ';'])
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .filter(|&n| n >= 1 && n <= total_pages)
            .collect(),
    };

    if pages_to_rotate.is_empty() {
        return Err(PdfError::InvalidParameter(
            "No valid page numbers provided to rotate".to_string(),
        ));
    }

    let pages = doc.get_pages();
    for &page_num in &pages_to_rotate {
        if let Some(&page_id) = pages.get(&page_num)
            && let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                page_dict.set("Rotate", Object::Integer(normalized as i64));
            }
    }

    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::minimal_pdf_bytes;

    #[tokio::test]
    async fn test_rotate_single_page() {
        let data = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = rotate_pdf(input, 90, None).await.unwrap();
        assert!(!result.is_empty());

        // Reload and verify Rotate key was set
        let doc = Document::load_mem(&result).unwrap();
        let pages = doc.get_pages();
        let page_id = pages.get(&1).unwrap();
        let dict = doc.get_dictionary(*page_id).unwrap();
        let rotate = dict.get(b"Rotate").unwrap();
        assert_eq!(*rotate, Object::Integer(90));
    }

    #[tokio::test]
    async fn test_rotate_normalized_negative() {
        let data = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        // -90 should normalize to 270
        let result = rotate_pdf(input, -90, None).await.unwrap();

        let doc = Document::load_mem(&result).unwrap();
        let pages = doc.get_pages();
        let page_id = pages.get(&1).unwrap();
        let dict = doc.get_dictionary(*page_id).unwrap();
        let rotate = dict.get(b"Rotate").unwrap();
        assert_eq!(*rotate, Object::Integer(270));
    }

    #[tokio::test]
    async fn test_rotate_specific_pages() {
        // Create a 2-page PDF by cloning the helper
        let data = minimal_pdf_bytes();
        let input1 = PdfInput::Bytes {
            data: data.clone(),
            mime_type: "application/pdf".to_string(),
            filename: None,
        };
        let input2 = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        use crate::merge::merge_pdfs;
        let merged = merge_pdfs(vec![input1, input2], None).await.unwrap();

        let input = PdfInput::Bytes {
            data: merged,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        // Rotate only page 2
        let result = rotate_pdf(input, 180, Some("2")).await.unwrap();

        let doc = Document::load_mem(&result).unwrap();
        let pages = doc.get_pages();

        // Page 1 should NOT have Rotate
        let dict1 = doc.get_dictionary(*pages.get(&1).unwrap()).unwrap();
        assert!(dict1.get(b"Rotate").is_err());

        // Page 2 should have Rotate = 180
        let dict2 = doc.get_dictionary(*pages.get(&2).unwrap()).unwrap();
        let rotate = dict2.get(b"Rotate").unwrap();
        assert_eq!(*rotate, Object::Integer(180));
    }

    #[tokio::test]
    async fn test_rotate_invalid_angle() {
        let data = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = rotate_pdf(input, 45, None).await;
        assert!(result.is_err());
    }
}
