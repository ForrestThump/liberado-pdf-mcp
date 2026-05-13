use lopdf::{Document, Object};
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};
use serde_json::json;

#[derive(Default)]
struct Meta {
    title: String,
    author: String,
    subject: String,
    creator: String,
    producer: String,
    creation_date: String,
    mod_date: String,
}

impl Meta {
    fn from_dict(dict: &lopdf::Dictionary) -> Self {
        let mut m = Self::default();
        if let Ok(val) = dict.get(b"Title") { m.title = text_from_object(val); }
        if let Ok(val) = dict.get(b"Author") { m.author = text_from_object(val); }
        if let Ok(val) = dict.get(b"Subject") { m.subject = text_from_object(val); }
        if let Ok(val) = dict.get(b"Creator") { m.creator = text_from_object(val); }
        if let Ok(val) = dict.get(b"Producer") { m.producer = text_from_object(val); }
        if let Ok(val) = dict.get(b"CreationDate") { m.creation_date = text_from_object(val); }
        if let Ok(val) = dict.get(b"ModDate") { m.mod_date = text_from_object(val); }
        m
    }
}

/// Extract metadata information from a PDF document.
///
/// Returns a JSON object with keys: page_count, file_size_bytes, version,
/// title, author, subject, creator, producer, creation_date, mod_date, is_encrypted.
pub async fn pdf_info(input: PdfInput) -> PdfResult<serde_json::Value> {
    let bytes = input.into_bytes().await?;
    let file_size_bytes = bytes.len();

    let doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let page_count = doc.get_pages().len();

    // Read PDF version
    let version = doc.version.to_string();

    let mut meta = Meta::default();

    if let Ok(info_obj) = doc.trailer.get(b"Info") {
        match info_obj {
            Object::Reference(id) => {
                if let Some(Object::Dictionary(info_dict)) = doc.objects.get(id) {
                    meta = Meta::from_dict(info_dict);
                }
            }
            Object::Dictionary(dict) => {
                meta = Meta::from_dict(dict);
            }
            _ => {}
        }
    }

    let is_encrypted = doc.encryption_state.is_some();

    Ok(json!({
        "page_count": page_count,
        "file_size_bytes": file_size_bytes,
        "version": version,
        "title": meta.title,
        "author": meta.author,
        "subject": meta.subject,
        "creator": meta.creator,
        "producer": meta.producer,
        "creation_date": meta.creation_date,
        "mod_date": meta.mod_date,
        "is_encrypted": is_encrypted,
    }))
}

/// Extract text from an Object, handling String, UnicodeString, and Name variants.
fn text_from_object(obj: &Object) -> String {
    match obj {
        Object::String(bytes, _) => decode_pdf_string(bytes),
        Object::Name(bytes) => String::from_utf8_lossy(bytes).to_string(),
        Object::Null => String::new(),
        _ => format!("{obj:?}"),
    }
}

/// Decode a PDF string, which is either UTF-16BE (BOM \xFE\xFF) or PDFDocEncoding.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16BE: pairs of bytes after the BOM
        let words: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        // PDFDocEncoding is ASCII-compatible for the lower 128 code points,
        // which covers the vast majority of real-world metadata.
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Object, Stream};

    fn pdf_with_info(title: &str, author: &str) -> Vec<u8> {
        let mut doc = Document::new();
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content_id = doc.new_object_id();
        let info_id = doc.new_object_id();

        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"BT /F1 12 Tf 100 700 Td (I) Tj ET".to_vec(),
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

        // Info dictionary
        let mut info = Dictionary::new();
        info.set(
            "Title",
            Object::String(title.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
        info.set(
            "Author",
            Object::String(author.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
        doc.objects.insert(info_id, Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[tokio::test]
    async fn test_pdf_info_basic() {
        let data = pdf_with_info("TestDoc", "TestAuthor");
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = pdf_info(input).await.unwrap();

        assert_eq!(result["page_count"], 1);
        assert!(result["file_size_bytes"].as_u64().unwrap() > 0);
        assert_eq!(result["title"], "TestDoc");
        assert_eq!(result["author"], "TestAuthor");
    }

    #[tokio::test]
    async fn test_pdf_info_no_metadata() {
        // Create a PDF without /Info in trailer
        let mut doc = Document::new();
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content_id = doc.new_object_id();

        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"BT /F1 12 Tf 100 700 Td (N) Tj ET".to_vec(),
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

        let input = PdfInput::Bytes {
            data: buf,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = pdf_info(input).await.unwrap();

        assert_eq!(result["page_count"], 1);
        assert_eq!(result["title"], "");
        assert_eq!(result["author"], "");
        assert_eq!(result["is_encrypted"], false);
    }

    #[tokio::test]
    async fn test_pdf_info_file_size() {
        let data = pdf_with_info("SizeTest", "Author");
        let size = data.len();
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: None,
        };

        let result = pdf_info(input).await.unwrap();

        assert_eq!(result["file_size_bytes"].as_u64().unwrap() as usize, size);
    }
}
