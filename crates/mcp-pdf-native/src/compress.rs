use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfResult};

/// Compress a PDF document using lopdf's built-in compression (FlateDecode).
///
/// `level` controls compression:
///   - `None`: apply compression (single pass)
///   - `Some(0)`: no compression
///   - `Some(1)`: single pass of compression
///   - `Some(2+)`: same as `Some(1)` — multiple passes are idempotent so only one is done
pub async fn compress_pdf(bytes: Vec<u8>, level: Option<u8>) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let passes = match level {
        None => 1,        // Default: compress
        Some(0) => 0,     // Explicitly no compression
        Some(_) => 1,     // Any positive level: compress (once)
    };

    for _ in 0..passes {
        doc.compress();
    }

    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Object, Stream};

    /// Create a PDF with an uncompressed stream that can be compressed.
    fn uncompressed_pdf_bytes() -> Vec<u8> {
        let mut doc = Document::new();
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content_id = doc.new_object_id();

        // Content stream stored without compression
        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"BT /F1 12 Tf 100 700 Td (Compress me) Tj ET".to_vec(),
            )),
        );

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Array(vec![Object::Reference(content_id)]));
        doc.objects.insert(page_id, Object::Dictionary(page));

        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set("Count", Object::Integer(1));
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        doc.objects.insert(catalog_id, Object::Dictionary(catalog));

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[tokio::test]
    async fn test_compress_pdf() {
        let data = uncompressed_pdf_bytes();
        let original_size = data.len();

        let result = compress_pdf(data, Some(1)).await.unwrap();
        assert!(!result.is_empty());

        // The compressed result should be a valid PDF
        let doc = Document::load_mem(&result);
        assert!(doc.is_ok());

        // Compression should typically reduce size (or at least not increase it much)
        // Note: very small PDFs may not compress well
        assert!(
            result.len() <= original_size || result.len() < original_size + 100,
            "Compressed size {} should not exceed original {} significantly",
            result.len(),
            original_size
        );
    }

    #[tokio::test]
    async fn test_compress_no_level() {
        let data = uncompressed_pdf_bytes();

        // level=None should apply compression
        let result = compress_pdf(data, None).await.unwrap();
        assert!(!result.is_empty());
        let doc = Document::load_mem(&result);
        assert!(doc.is_ok());
    }

    #[tokio::test]
    async fn test_compress_level_zero() {
        let data = uncompressed_pdf_bytes();
        let original_size = data.len();

        // level=0 should output uncompressed (same or very close to original)
        let result = compress_pdf(data, Some(0)).await.unwrap();
        // Size should be close to original (may differ slightly due to serialization)
        let diff = if result.len() > original_size {
            result.len() - original_size
        } else {
            original_size - result.len()
        };
        // Allow some small difference due to serialization variation
        assert!(
            diff < 100,
            "Size difference {} is too large for level=0",
            diff
        );
    }

    #[tokio::test]
    async fn test_compress_multiple_passes() {
        let data = uncompressed_pdf_bytes();

        // level=3 should not crash or produce invalid output
        let result = compress_pdf(data, Some(3)).await.unwrap();
        assert!(!result.is_empty());
        let doc = Document::load_mem(&result);
        assert!(doc.is_ok());
    }
}
