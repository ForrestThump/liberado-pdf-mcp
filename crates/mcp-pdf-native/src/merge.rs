use lopdf::{Dictionary, Document, Object, ObjectId};
use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

/// Merge multiple PDFs into a single PDF document.
/// Inputs are resolved to bytes, then merged in order.
pub async fn merge_pdfs(
    inputs: Vec<PdfInput>,
    sort_type: Option<&str>,
) -> PdfResult<Vec<u8>> {
    if inputs.is_empty() {
        return Err(PdfError::InvalidParameter(
            "No PDF files provided".to_string(),
        ));
    }

    // Extract filenames BEFORE consuming inputs
    let filenames: Vec<String> = inputs.iter().map(|i| i.filename().to_string()).collect();

    let mut docs: Vec<Document> = Vec::new();
    for input in inputs {
        let bytes = input.into_bytes().await?;
        let doc = Document::load_mem(&bytes)
            .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;
        docs.push(doc);
    }

    // Sort by filename if requested
    if let Some(sort) = sort_type {
        let mut indexed: Vec<(Document, String)> = docs
            .into_iter()
            .zip(filenames.into_iter())
            .collect();

        match sort {
            "alphabetical" => indexed.sort_by(|a, b| a.1.cmp(&b.1)),
            "reverseAlphabetical" => indexed.sort_by(|a, b| b.1.cmp(&a.1)),
            _ => {} // "orderProvided" or unknown — keep original order
        }

        docs = indexed.into_iter().map(|(doc, _)| doc).collect();
    }

    if docs.len() == 1 {
        let mut buf = Vec::new();
        docs[0]
            .save_to(&mut buf)
            .map_err(PdfError::Io)?;
        return Ok(buf);
    }

    // Create result document — empty, no starting objects
    let mut result = Document::new();
    let mut next_id: u32 = 1;

    // Renumber each source doc to avoid ID collisions, then copy all objects
    for doc in &mut docs {
        doc.renumber_objects_with(next_id);

        for (&id, object) in &doc.objects {
            debug_assert!(
                !result.objects.contains_key(&id),
                "Object ID collision detected: {id:?}"
            );
            result.objects.insert(id, object.clone());
        }

        result.max_id = result.max_id.max(doc.max_id);
        next_id = result.max_id + 1;
    }

    // Collect all page ObjectIds from every source document
    let mut all_page_ids: Vec<ObjectId> = Vec::new();
    for doc in &docs {
        for (_, page_id) in doc.get_pages() {
            all_page_ids.push(page_id);
        }
    }

    // Build the page tree in the result document
    let pages_dict_id = result.new_object_id();
    let catalog_id = result.new_object_id();

    // Pages dictionary
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", Object::Integer(all_page_ids.len() as i64));

    let kids: Vec<Object> = all_page_ids
        .iter()
        .map(|&id| Object::Reference(id))
        .collect();
    pages_dict.set("Kids", Object::Array(kids));

    // Update Parent reference on each page to point to our new Pages dict
    for &page_id in &all_page_ids {
        if let Ok(page_dict) = result.get_dictionary_mut(page_id) {
            page_dict.set("Parent", Object::Reference(pages_dict_id));
        }
    }

    result
        .objects
        .insert(pages_dict_id, Object::Dictionary(pages_dict));

    // Catalog
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_dict_id));
    result
        .objects
        .insert(catalog_id, Object::Dictionary(catalog));

    // Trailer Root
    result.trailer.set("Root", Object::Reference(catalog_id));

    result.compress();

    let mut buf = Vec::new();
    result
        .save_modern(&mut buf)
        .map_err(PdfError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::minimal_pdf_bytes;

    #[tokio::test]
    async fn test_merge_single_pdf() {
        let data = minimal_pdf_bytes();
        let input = PdfInput::Bytes {
            data,
            mime_type: "application/pdf".to_string(),
            filename: Some("test.pdf".to_string()),
        };
        let result = merge_pdfs(vec![input], None).await;
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // Result should be a valid PDF
        let loaded = Document::load_mem(&bytes);
        assert!(loaded.is_ok());
        assert_eq!(loaded.unwrap().get_pages().len(), 1);
    }

    #[tokio::test]
    async fn test_merge_multiple_pdfs() {
        let pdf1 = minimal_pdf_bytes();
        let pdf2 = minimal_pdf_bytes();
        let inputs = vec![
            PdfInput::Bytes {
                data: pdf1,
                mime_type: "application/pdf".to_string(),
                filename: Some("a.pdf".to_string()),
            },
            PdfInput::Bytes {
                data: pdf2,
                mime_type: "application/pdf".to_string(),
                filename: Some("b.pdf".to_string()),
            },
        ];
        let result = merge_pdfs(inputs, None).await;
        assert!(result.is_ok());
        let merged_bytes = result.unwrap();

        let merged_doc = Document::load_mem(&merged_bytes);
        assert!(merged_doc.is_ok(), "Merged result should be valid PDF");
        assert_eq!(merged_doc.unwrap().get_pages().len(), 2);
    }

    #[tokio::test]
    async fn test_merge_empty_inputs() {
        let result = merge_pdfs(vec![], None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_merge_three_pdfs() {
        let pdf = minimal_pdf_bytes();
        let inputs = vec![
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
        ];
        let result = merge_pdfs(inputs, None).await;
        assert!(result.is_ok());
        let merged_doc = Document::load_mem(&result.unwrap()).unwrap();
        assert_eq!(merged_doc.get_pages().len(), 3);
    }
}
