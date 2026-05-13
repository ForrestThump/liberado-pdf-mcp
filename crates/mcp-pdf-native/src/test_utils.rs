use lopdf::{Dictionary, Document, Object, Stream};

pub fn minimal_pdf_bytes() -> Vec<u8> {
    let mut doc = Document::new();
    let catalog_id = doc.new_object_id();
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();

    doc.objects.insert(
        content_id,
        Object::Stream(Stream::new(
            Dictionary::new(),
            b"BT /F1 12 Tf 100 700 Td (X) Tj ET".to_vec(),
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

pub fn pdf_with_extractable_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new();

    let font_id = doc.new_object_id();
    let fonts_id = doc.new_object_id();
    let resources_id = doc.new_object_id();
    let content_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let pages_id = doc.new_object_id();
    let catalog_id = doc.new_object_id();

    let mut font_dict = Dictionary::new();
    font_dict.set("Type", Object::Name(b"Font".to_vec()));
    font_dict.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_dict.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    doc.objects.insert(font_id, Object::Dictionary(font_dict));

    let mut fonts_dict = Dictionary::new();
    fonts_dict.set("F1", Object::Reference(font_id));
    doc.objects.insert(fonts_id, Object::Dictionary(fonts_dict));

    let mut resources = Dictionary::new();
    resources.set("Font", Object::Reference(fonts_id));
    doc.objects.insert(resources_id, Object::Dictionary(resources));

    // Escape PDF string delimiters so arbitrary text doesn't break the content stream.
    let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let content = format!("BT /F1 12 Tf 100 700 Td ({escaped}) Tj ET");
    doc.objects.insert(
        content_id,
        Object::Stream(Stream::new(Dictionary::new(), content.into_bytes())),
    );

    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set("Contents", Object::Array(vec![Object::Reference(content_id)]));
    page.set("Resources", Object::Reference(resources_id));
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
