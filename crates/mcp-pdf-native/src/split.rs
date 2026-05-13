use std::collections::HashSet;
use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Split a PDF at given page numbers (1-indexed).
/// `page_numbers` is a comma/semicolon-separated list of page numbers
/// marking the START of each new part.
/// E.g. for a 5-page PDF: "3" gives [1-2], [3-5]; "2,4" gives [1], [2-3], [4-5].
///
/// Returns a Vec of PDF byte buffers (one per split part).
pub async fn split_pdf(input: PdfInput, page_numbers: &str) -> PdfResult<Vec<Vec<u8>>> {
    let bytes = input.into_bytes().await?;
    let doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let total_pages = doc.get_pages().len() as u32;
    if total_pages == 0 {
        return Err(PdfError::InvalidParameter(
            "PDF has no pages".to_string(),
        ));
    }

    // Parse split points (1-indexed user input)
    // These mark the START page of each new segment
    let mut parse_errors: Vec<String> = Vec::new();
    let mut split_points: Vec<u32> = Vec::new();
    for token in page_numbers.split([',', ';']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        match token.parse::<u32>() {
            Ok(n) if n > 1 && n <= total_pages => split_points.push(n),
            Ok(1) => {
                parse_errors.push(
                    "Page 1 is not a valid split point (would create empty first segment)"
                        .to_string(),
                );
            }
            Ok(n) => {
                parse_errors.push(format!("Page {n} is out of range (1-{total_pages})"));
            }
            Err(_) => {
                parse_errors.push(format!("'{token}' is not a valid page number"));
            }
        }
    }
    if !parse_errors.is_empty() {
        return Err(PdfError::InvalidParameter(format!(
            "Invalid split points: {}",
            parse_errors.join("; ")
        )));
    }

    if split_points.is_empty() {
        return Err(PdfError::InvalidParameter(format!(
            "Invalid split points: '{page_numbers}'. {}",
            if total_pages < 2 {
                "PDF has too few pages to split (minimum 2 pages required).".to_string()
            } else {
                format!("Must be 2-{total_pages}")
            }
        )));
    }

    // Deduplicate and sort
    split_points.sort_unstable();
    split_points.dedup();

    // Build ranges based on split points as segment starts
    let mut ranges: Vec<(u32, u32)> = Vec::new();

    // First segment: pages 1..(first_split_point - 1)
    ranges.push((1, split_points[0] - 1));

    // Middle segments: between consecutive split points
    for i in 0..split_points.len() - 1 {
        ranges.push((split_points[i], split_points[i + 1] - 1));
    }

    // Last segment: from last split point to total_pages
    ranges.push((*split_points.last().unwrap(), total_pages));

    // Drop the parsed doc — extract_pages_from_bytes reloads from bytes per
    // range so only one Document lives in memory at a time.
    drop(doc);

    let mut results = Vec::new();
    for (range_start, range_end) in ranges {
        if range_start > range_end {
            continue;
        }
        let pages_to_keep: Vec<u32> = (range_start..=range_end).collect();
        let part = extract_pages_from_bytes(&bytes, &pages_to_keep)?;
        results.push(part);
    }

    Ok(results)
}

/// Extract specific 1-indexed pages from raw PDF bytes.
///
/// Parses, removes pages not in `pages`, and serialises. By accepting bytes
/// rather than a pre-parsed `&Document` each call reloads independently, so
/// callers can iterate ranges without holding N clones of the document tree.
pub fn extract_pages_from_bytes(bytes: &[u8], pages: &[u32]) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;
    let total_pages = doc.get_pages().len() as u32;

    let keep_set: HashSet<u32> = pages.iter().copied().collect();
    let to_remove: Vec<u32> = (1..=total_pages)
        .filter(|p| !keep_set.contains(p))
        .collect();

    if !to_remove.is_empty() {
        doc.delete_pages(&to_remove);
    }

    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(PdfError::Io)?;
    Ok(buf)
}

/// Convenience wrapper for callers that already have a parsed `Document`.
pub fn extract_pages_sync(doc: &Document, pages: &[u32]) -> PdfResult<Vec<u8>> {
    let total_pages = doc.get_pages().len() as u32;
    let keep_set: HashSet<u32> = pages.iter().copied().collect();
    let to_remove: Vec<u32> = (1..=total_pages)
        .filter(|p| !keep_set.contains(p))
        .collect();

    let mut new_doc = doc.clone();
    if !to_remove.is_empty() {
        new_doc.delete_pages(&to_remove);
    }

    let mut buf = Vec::new();
    new_doc.save_to(&mut buf).map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::merge_pdfs;
    use crate::test_utils::minimal_pdf_bytes;

    fn make_input(data: Vec<u8>) -> PdfInput {
        PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        }
    }

    #[tokio::test]
    async fn test_split_valid() {
        // Create a 3-page PDF by merging 3 copies
        let pdf = minimal_pdf_bytes();
        let inputs = vec![
            make_input(pdf.clone()),
            make_input(pdf.clone()),
            make_input(pdf),
        ];
        let merged = merge_pdfs(inputs, None).await.unwrap();

        let input = make_input(merged);
        let parts = split_pdf(input, "2").await.unwrap();
        assert_eq!(parts.len(), 2);

        // Each part should be a valid PDF
        for part in &parts {
            let doc = Document::load_mem(part);
            assert!(doc.is_ok(), "Each split part must be a valid PDF");
        }
    }

    #[tokio::test]
    async fn test_split_three_ways() {
        let pdf = minimal_pdf_bytes();
        let inputs = vec![
            make_input(pdf.clone()),
            make_input(pdf.clone()),
            make_input(pdf),
        ];
        let merged = merge_pdfs(inputs, None).await.unwrap();

        let input = make_input(merged);
        let parts = split_pdf(input, "2,3").await.unwrap();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_extract_pages_sync() {
        let pdf = minimal_pdf_bytes();
        let doc = Document::load_mem(&pdf).unwrap();
        let result = extract_pages_sync(&doc, &[1]).unwrap();
        assert!(!result.is_empty());
        let extracted = Document::load_mem(&result).unwrap();
        assert_eq!(extracted.get_pages().len(), 1);
    }

    #[tokio::test]
    async fn test_split_invalid_page_numbers() {
        let pdf = minimal_pdf_bytes();
        let input = make_input(pdf);
        let result = split_pdf(input, "").await;
        assert!(result.is_err());
    }
}
