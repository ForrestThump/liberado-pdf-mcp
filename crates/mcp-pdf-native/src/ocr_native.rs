use lopdf::{Document, Object};

use mcp_pdf_core::{PdfError, PdfInput, PdfResult};

use image::DynamicImage;

/// Perform OCR on a scanned PDF by extracting embedded page images
/// and running Tesseract OCR on each one.
///
/// If the PDF already contains extractable text (born-digital, not scanned),
/// the existing text is returned without invoking Tesseract.
///
/// Page ordering is preserved via lopdf's BTreeMap iteration.
pub async fn ocr_pdf(input: PdfInput, language: &str) -> PdfResult<String> {
    let bytes = input.into_bytes().await?;
    let doc = Document::load_mem(&bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse PDF: {e}")))?;

    let total_pages = doc.get_pages().len();
    if total_pages == 0 {
        return Ok(String::new());
    }

    let page_numbers: Vec<u32> = (1..=total_pages as u32).collect();
    let existing_text = doc.extract_text(&page_numbers).unwrap_or_default();
    if !existing_text.trim().is_empty() {
        return Ok(format!("Extracted text (no OCR needed):\n\n{existing_text}"));
    }

    let tessdata_dir = get_tessdata_dir();
    let mut results: Vec<String> = Vec::new();

    for (page_num, _) in doc.get_pages() {
        let images = extract_page_images(&doc, page_num)?;
        if images.is_empty() {
            results.push(format!("Page {page_num}: [No embedded images found]"));
            continue;
        }

        let mut page_text = String::new();
        for img in &images {
            match ocr_image(img, language, &tessdata_dir) {
                Ok(text) => page_text.push_str(&text),
                Err(e) => {
                    tracing::warn!("OCR failed on page {page_num}: {e}");
                }
            }
            page_text.push('\n');
        }

        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            results.push(format!("Page {page_num}: [No text recognized]"));
        } else {
            results.push(format!("Page {page_num}:\n{trimmed}"));
        }
    }

    Ok(results.join("\n\n"))
}

/// Extract embedded images from a PDF page's XObject resources.
fn extract_page_images(doc: &Document, page_num: u32) -> PdfResult<Vec<DynamicImage>> {
    let page_id = doc
        .get_pages()
        .get(&page_num)
        .copied()
        .ok_or_else(|| PdfError::Manipulation(format!("Page {page_num} not found")))?;

    let page = doc
        .get_object(page_id)
        .map_err(|e| PdfError::Parse(format!("Failed to get page {page_num}: {e}")))?;

    let page_dict = page
        .as_dict()
        .map_err(|_| PdfError::Manipulation(format!("Page {page_num} is not a dictionary")))?;

    let xobjects = get_xobject_dict(doc, page_dict)?;
    if xobjects.is_empty() {
        return Ok(Vec::new());
    }

    let mut images = Vec::new();
    for (_, obj) in &xobjects {
        let stream = match obj {
            Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_stream().ok()),
            other => other.as_stream().ok(),
        };

        if let Some(stream) = stream {
            if is_image_stream(stream) {
                match stream_to_image(stream) {
                    Ok(img) => images.push(img),
                    Err(e) => {
                        tracing::warn!("Failed to decode image on page {page_num}: {e}");
                    }
                }
            }
        }
    }

    Ok(images)
}

/// Check if a stream XObject is an image subtype.
fn is_image_stream(stream: &lopdf::Stream) -> bool {
    stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n == b"Image")
        .unwrap_or(false)
}

/// Get the XObject dictionary from a page's resources.
fn get_xobject_dict(doc: &Document, page_dict: &lopdf::Dictionary) -> PdfResult<lopdf::Dictionary> {
    let resources = page_dict.get(b"Resources").ok();

    let xobject = match resources {
        Some(Object::Dictionary(res_dict)) => res_dict.get(b"XObject").ok(),
        Some(Object::Reference(id)) => {
            let obj = doc
                .get_object(*id)
                .map_err(|e| PdfError::Parse(format!("Failed to resolve page resources: {e}")))?;
            obj.as_dict().ok().and_then(|d| d.get(b"XObject").ok())
        }
        _ => None,
    };

    match xobject {
        Some(Object::Dictionary(d)) => Ok(d.clone()),
        Some(Object::Reference(id)) => {
            let obj = doc
                .get_object(*id)
                .map_err(|e| PdfError::Parse(format!("Failed to resolve XObject dict: {e}")))?;
            Ok(obj
                .as_dict()
                .map_err(|_| PdfError::Manipulation("XObject reference is not a dictionary".into()))?
                .clone())
        }
        _ => Ok(lopdf::Dictionary::new()),
    }
}

/// Decode a PDF image stream into a `DynamicImage`.
fn stream_to_image(stream: &lopdf::Stream) -> PdfResult<DynamicImage> {
    let filter = stream.dict.get(b"Filter").ok();

    if let Some(Object::Name(name)) = filter {
        if name == b"DCTDecode" || name == b"JPXDecode" {
            return image::load_from_memory(&stream.content)
                .map_err(|e| PdfError::Other(format!("Failed to decode JPEG image: {e}")));
        }
    }

    let mut s = stream.clone();
    match s.decompress() {
        Ok(()) => {
            let data = s.content;
            if let Ok(img) = image::load_from_memory(&data) {
                return Ok(img);
            }
            build_image_from_raw(&stream.dict, data)
        }
        Err(_) => image::load_from_memory(&stream.content)
            .map_err(|e| PdfError::Other(format!("Failed to decode image stream: {e}"))),
    }
}

/// Build an image from raw decompressed pixel data and stream metadata.
fn build_image_from_raw(dict: &lopdf::Dictionary, data: Vec<u8>) -> PdfResult<DynamicImage> {
    let width = dict
        .get(b"Width")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as u32;
    let height = dict
        .get(b"Height")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as u32;

    if width == 0 || height == 0 {
        return Err(PdfError::Other("Image stream missing Width/Height".into()));
    }

    let bpc = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8);
    let color_space = dict.get(b"ColorSpace").ok();

    match (color_space, bpc) {
        (Some(Object::Name(n)), 8) if n == b"DeviceRGB" => {
            let data_len = data.len();
            let img = image::ImageBuffer::from_raw(width, height, data).ok_or_else(|| {
                PdfError::Other(format!(
                    "Failed to construct RGB image ({}x{}, data len {})",
                    width, height, data_len
                ))
            })?;
            Ok(DynamicImage::ImageRgb8(img))
        }
        (Some(Object::Name(n)), 1) if n == b"DeviceGray" => {
            let byte_len = ((width * height + 7) / 8) as usize;
            let mut gray: Vec<u8> = Vec::with_capacity((width * height) as usize);
            for i in 0..(width * height) as usize {
                let byte_idx = i / 8;
                let bit_idx = 7 - (i % 8);
                if byte_idx < data.len().min(byte_len) {
                    let pixel = if (data[byte_idx] >> bit_idx) & 1 == 1 {
                        255u8
                    } else {
                        0u8
                    };
                    gray.push(pixel);
                }
            }
            let img = image::ImageBuffer::from_raw(width, height, gray).ok_or_else(|| {
                PdfError::Other("Failed to construct bilevel image".into())
            })?;
            Ok(DynamicImage::ImageLuma8(img))
        }
        (Some(Object::Name(n)), 8) if n == b"DeviceGray" => {
            let data_len = data.len();
            let img = image::ImageBuffer::from_raw(width, height, data).ok_or_else(|| {
                PdfError::Other(format!(
                    "Failed to construct grayscale image ({}x{}, data len {})",
                    width, height, data_len
                ))
            })?;
            Ok(DynamicImage::ImageLuma8(img))
        }
        (Some(Object::Name(n)), _) if n == b"ICCBased" => {
            let expected = (width * height * 3) as usize;
            if data.len() >= expected {
                let rgb = data[..expected].to_vec();
                let img = image::ImageBuffer::from_raw(width, height, rgb).ok_or_else(|| {
                    PdfError::Other("Failed to construct ICC-based image".into())
                })?;
                Ok(DynamicImage::ImageRgb8(img))
            } else {
                Err(PdfError::Other(format!(
                    "ICCBased image data too short: {} < {}",
                    data.len(),
                    expected
                )))
            }
        }
        _ => Err(PdfError::Other(format!(
            "Unsupported image format: color_space={color_space:?}, bpc={bpc}, dimensions={width}x{height}"
        ))),
    }
}

fn get_tessdata_dir() -> String {
    std::env::var("TESSDATA_PREFIX").unwrap_or_else(|_| "/usr/share/tessdata".to_string())
}

fn ocr_image(img: &DynamicImage, language: &str, tessdata_dir: &str) -> PdfResult<String> {
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();
    let raw = rgb.into_raw();
    let stride = 3 * width as i32;

    let api = kreuzberg_tesseract::TesseractAPI::new()
        .map_err(|e| PdfError::Other(format!("Failed to create Tesseract API: {e}")))?;

    api.init(tessdata_dir, language)
        .map_err(|e| PdfError::Other(format!("Failed to initialize Tesseract (check TESSDATA_PREFIX): {e}")))?;

    api.set_variable("tessedit_pageseg_mode", "1")
        .map_err(|e| PdfError::Other(format!("Failed to set page segmentation mode: {e}")))?;

    api.set_image(&raw, width as i32, height as i32, 3, stride)
        .map_err(|e| PdfError::Other(format!("Failed to set image data: {e}")))?;

    let text = api
        .get_utf8_text()
        .map_err(|e| PdfError::Other(format!("OCR recognition failed: {e}")))?;

    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Object, Stream};

    fn tiny_jpeg_bytes() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
            0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43,
            0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09,
            0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12,
            0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
            0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29,
            0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01,
            0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00,
            0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
            0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02,
            0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x22, 0x31, 0x06, 0x41, 0x51,
            0x61, 0x07, 0x13, 0x71, 0x81, 0x91, 0xA1, 0x08, 0x14, 0x23, 0x42, 0xB1,
            0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A,
            0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34,
            0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
            0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64,
            0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
            0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93,
            0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6,
            0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9,
            0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3,
            0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5,
            0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
            0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F,
            0x00, 0x7B, 0x94, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xFF, 0xD9,
        ]
    }

    fn pdf_with_embedded_jpeg(jpeg_data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut doc = Document::new();
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let xobject_id = doc.new_object_id();
        let xobjects_id = doc.new_object_id();
        let resources_id = doc.new_object_id();

        let mut img_dict = Dictionary::new();
        img_dict.set("Type", Object::Name(b"XObject".to_vec()));
        img_dict.set("Subtype", Object::Name(b"Image".to_vec()));
        img_dict.set("Width", Object::Integer(width as i64));
        img_dict.set("Height", Object::Integer(height as i64));
        img_dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        img_dict.set("BitsPerComponent", Object::Integer(8));
        img_dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));

        doc.objects.insert(
            xobject_id,
            Object::Stream(lopdf::Stream::new(img_dict, jpeg_data.to_vec())),
        );

        let mut xobj_dict = Dictionary::new();
        xobj_dict.set("Im0", Object::Reference(xobject_id));
        doc.objects.insert(xobjects_id, Object::Dictionary(xobj_dict));

        let mut resources = Dictionary::new();
        resources.set("XObject", Object::Reference(xobjects_id));
        doc.objects.insert(resources_id, Object::Dictionary(resources));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
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

    #[test]
    fn test_is_image_stream_positive() {
        let mut dict = Dictionary::new();
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        let stream = lopdf::Stream::new(dict, vec![1, 2, 3]);
        assert!(is_image_stream(&stream));
    }

    #[test]
    fn test_is_image_stream_negative() {
        let mut dict = Dictionary::new();
        dict.set("Subtype", Object::Name(b"Font".to_vec()));
        let stream = lopdf::Stream::new(dict, vec![1, 2, 3]);
        assert!(!is_image_stream(&stream));
    }

    #[test]
    fn test_is_image_stream_no_subtype() {
        let dict = Dictionary::new();
        let stream = lopdf::Stream::new(dict, vec![1, 2, 3]);
        assert!(!is_image_stream(&stream));
    }

    #[test]
    fn test_stream_to_image_dct_decode() {
        let jpeg = tiny_jpeg_bytes();
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", Object::Integer(1));
        dict.set("Height", Object::Integer(1));
        dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        dict.set("BitsPerComponent", Object::Integer(8));
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));

        let stream = lopdf::Stream::new(dict, jpeg);
        let result = stream_to_image(&stream);
        assert!(result.is_ok(), "DCTDecode image should decode, got: {:?}", result.err());
        let img = result.unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }

    #[test]
    fn test_stream_to_image_raw_rgb() {
        let data = vec![255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", Object::Integer(2));
        dict.set("Height", Object::Integer(2));
        dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        dict.set("BitsPerComponent", Object::Integer(8));

        let result = build_image_from_raw(&dict, data);
        assert!(result.is_ok(), "Raw RGB should decode, got: {:?}", result.err());
        let img = result.unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
    }

    #[test]
    fn test_stream_to_image_gray() {
        let data = vec![128u8, 200, 64, 255];
        let mut dict = Dictionary::new();
        dict.set("Width", Object::Integer(2));
        dict.set("Height", Object::Integer(2));
        dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
        dict.set("BitsPerComponent", Object::Integer(8));

        let result = build_image_from_raw(&dict, data);
        assert!(result.is_ok(), "Grayscale should decode, got: {:?}", result.err());
        let img = result.unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
    }

    #[test]
    fn test_stream_to_image_missing_dimensions() {
        let dict = Dictionary::new();
        let result = build_image_from_raw(&dict, vec![]);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("missing"),
            "Should report missing dimensions"
        );
    }

    #[test]
    fn test_extract_page_images_finds_jpeg() {
        let jpeg = tiny_jpeg_bytes();
        let pdf_bytes = pdf_with_embedded_jpeg(&jpeg, 1, 1);
        let doc = Document::load_mem(&pdf_bytes).unwrap();

        let images = extract_page_images(&doc, 1).unwrap();
        assert_eq!(images.len(), 1, "Should find one embedded image");
        assert_eq!(images[0].width(), 1);
        assert_eq!(images[0].height(), 1);
    }

    #[test]
    fn test_extract_page_images_nonexistent_page() {
        let doc = Document::new();
        let result = extract_page_images(&doc, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_bilevel_image_1bit() {
        let data = vec![0b10000000, 0b01000000];
        let mut dict = Dictionary::new();
        dict.set("Width", Object::Integer(2));
        dict.set("Height", Object::Integer(2));
        dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
        dict.set("BitsPerComponent", Object::Integer(1));

        let result = build_image_from_raw(&dict, data);
        assert!(result.is_ok(), "Bilevel should decode, got: {:?}", result.err());
        let img = result.unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
    }
}
