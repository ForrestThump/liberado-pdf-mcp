use mcp_pdf_core::{format_pdf_response, format_search_results, PdfError, PdfInput, PdfOutput};
use mcp_pdf_native as native;
use turbomcp::prelude::*;

#[cfg(feature = "stirling-bridge")]
use mcp_pdf_stirling as stirling;

use crate::config::ServerConfig;

#[derive(Clone)]
pub struct PdfServer {
    pub config: ServerConfig,
}

#[server(name = "mcp-pdf-rs", version = "0.1.0")]
impl PdfServer {
    /// Merge multiple PDF files into a single PDF document.
    #[tool]
    async fn merge_pdfs(
        &self,
        #[description("Array of PDF files as base64 data URLs or file paths")]
        pdf_files: Vec<String>,
        #[description("Sort order: orderProvided (default), alphabetical, reverseAlphabetical")]
        sort_type: Option<String>,
    ) -> McpResult<String> {
        let inputs: Vec<PdfInput> = pdf_files
            .iter()
            .map(|s| parse_input(s))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let result = native::merge::merge_pdfs(inputs, sort_type.as_deref())
            .await
            .map_err(|e| map_tool_err("merge_pdfs", e))?;

        Ok(format_pdf_response(result, "Successfully merged PDFs"))
    }

    /// Split a PDF document at specified page numbers.
    #[tool]
    async fn split_pdf(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Comma-separated page numbers to split at (1-indexed)")]
        page_numbers: String,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let parts = native::split::split_pdf(input, &page_numbers)
            .await
            .map_err(|e| map_tool_err("split_pdf", e))?;

        let mut output = format!("Split PDF into {} parts:\n\n", parts.len());
        for (i, part) in parts.into_iter().enumerate() {
            let data_url = PdfOutput::pdf_data_url(part).to_mcp_response();
            output.push_str(&format!("## Part {}\n{data_url}\n\n", i + 1));
        }
        Ok(output)
    }

    /// Extract specific pages from a PDF document.
    #[tool]
    async fn extract_pages(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Page numbers to extract (1-indexed, e.g. [1, 3, 5])")]
        pages: Vec<u32>,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let result = native::extract::extract_pages(input, &pages)
            .await
            .map_err(|e| map_tool_err("extract_pages", e))?;

        Ok(format_pdf_response(result, "Successfully extracted pages"))
    }

    /// Remove specified pages from a PDF document.
    #[tool]
    async fn remove_pages(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Comma-separated page numbers to remove (1-indexed)")]
        pages_to_remove: String,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let result = native::remove::remove_pages(input, &pages_to_remove)
            .await
            .map_err(|e| map_tool_err("remove_pages", e))?;

        Ok(format_pdf_response(result, "Successfully removed pages"))
    }

    /// Rotate pages in a PDF document.
    #[tool]
    async fn rotate_pdf(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Rotation angle: 0, 90, 180, or 270")]
        angle: i32,
        #[description("Comma-separated page numbers to rotate (1-indexed). Omit to rotate all pages.")]
        page_numbers: Option<String>,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let result = native::rotate::rotate_pdf(input, angle, page_numbers.as_deref())
            .await
            .map_err(|e| map_tool_err("rotate_pdf", e))?;

        Ok(format_pdf_response(
            result,
            &format!("Rotated PDF by {angle} degrees"),
        ))
    }

    /// Compress a PDF document to reduce file size.
    #[tool]
    async fn compress_pdf(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Compression level (0-3, default: 1)")]
        level: Option<u8>,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let bytes = input.into_bytes().await
            .map_err(|e| map_tool_err("compress_pdf", e))?;
        let input_len = bytes.len();

        let result = native::compress::compress_pdf(bytes, level)
            .await
            .map_err(|e| map_tool_err("compress_pdf", e))?;

        let ratio = format!(" ({} → {} bytes)", input_len, result.len());

        Ok(format_pdf_response(result, &format!("Compressed PDF{ratio}")))
    }

    /// Extract plain text from all pages of a PDF document.
    #[tool]
    async fn extract_text(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let text = native::text::extract_text(input)
            .await
            .map_err(|e| map_tool_err("extract_text", e))?;

        if text.trim().is_empty() {
            Ok("No text content extracted from PDF.".to_string())
        } else {
            Ok(format!("Extracted text:\n\n{text}"))
        }
    }

    /// Get metadata about a PDF (page count, size, author, etc.).
    #[tool]
    async fn pdf_info(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
    ) -> McpResult<String> {
        let input = parse_input(&pdf_file)?;

        let info = native::info::pdf_info(input)
            .await
            .map_err(|e| map_tool_err("pdf_info", e))?;

        let page_count = info["page_count"].as_u64().unwrap_or(0);
        let file_size = info["file_size_bytes"].as_u64().unwrap_or(0);
        let title = info["title"].as_str().unwrap_or("");
        let author = info["author"].as_str().unwrap_or("");
        let subject = info["subject"].as_str().unwrap_or("");
        let version = info["version"].as_str().unwrap_or("");
        let is_encrypted = info["is_encrypted"].as_bool().unwrap_or(false);

        let mut output = format!(
            "PDF Info:\n- Pages: {page_count}\n- Size: {file_size} bytes\n- Version: {version}\n- Encrypted: {is_encrypted}"
        );
        if !title.is_empty() {
            output.push_str(&format!("\n- Title: {title}"));
        }
        if !author.is_empty() {
            output.push_str(&format!("\n- Author: {author}"));
        }
        if !subject.is_empty() {
            output.push_str(&format!("\n- Subject: {subject}"));
        }

        Ok(output)
    }

    /// Search for PDF files in a directory with pattern matching.
    #[tool]
    async fn search_pdfs(
        &self,
        #[description("Base directory to search in")]
        base_path: String,
        #[description("Glob or substring pattern for filenames")]
        pattern: Option<String>,
        #[description("Search subdirectories recursively (default: true)")]
        recursive: Option<bool>,
    ) -> McpResult<String> {
        let base = base_path;
        let pat = pattern.unwrap_or_default();
        let rec = recursive.unwrap_or(true);

        let results = tokio::task::spawn_blocking(move || {
            native::search::search_pdfs(&base, &pat, rec)
        })
        .await
        .map_err(|e| McpError::tool_execution_failed("search_pdfs", e.to_string()))?
        .map_err(|e| map_tool_err("search_pdfs", e))?;

        Ok(format_search_results(&results))
    }

    /// Find and merge PDFs in a specific order using filename patterns.
    #[tool]
    async fn merge_ordered(
        &self,
        #[description("Directory containing PDFs")]
        base_path: String,
        #[description("Filename patterns in desired merge order")]
        patterns: Vec<String>,
        #[description("Use fuzzy matching for filenames")]
        fuzzy_matching: Option<bool>,
    ) -> McpResult<String> {
        let base = base_path;
        let all_pdfs = tokio::task::spawn_blocking(move || {
            native::search::search_pdfs(&base, "", true)
        })
        .await
        .map_err(|e| McpError::tool_execution_failed("merge_ordered", e.to_string()))?
        .map_err(|e| map_tool_err("merge_ordered", e))?;

        let file_names: Vec<String> = all_pdfs.iter().map(|r| r.path.clone()).collect();
        let mut ordered_files: Vec<std::path::PathBuf> = Vec::new();

        for pattern in &patterns {
            if fuzzy_matching.unwrap_or(false) {
                let scored = native::search::fuzzy_match_pdfs(file_names.clone(), pattern);
                if let Some((best, _)) = scored.first() {
                    let p = std::path::PathBuf::from(best);
                    if !ordered_files.contains(&p) {
                        ordered_files.push(p);
                    }
                }
            } else {
                for path in &file_names {
                    if path.contains(pattern.as_str()) {
                        let p = std::path::PathBuf::from(path);
                        if !ordered_files.contains(&p) {
                            ordered_files.push(p);
                        }
                    }
                }
            }
        }

        let mut to_merge: Vec<PdfInput> = Vec::new();
        for file_path in ordered_files {
            to_merge.push(PdfInput::FilePath(file_path));
        }

        if to_merge.is_empty() {
            return Err(McpError::tool_execution_failed(
                "merge_ordered",
                "No matching PDFs found for the given patterns",
            ));
        }

        let result = native::merge::merge_pdfs(to_merge, None)
            .await
            .map_err(|e| map_tool_err("merge_ordered", e))?;

        Ok(format_pdf_response(
            result,
            "Successfully merged PDFs in specified order",
        ))
    }

    /// Find PDFs related by content (text extraction and word matching).
    #[tool]
    async fn find_related_pdfs(
        &self,
        #[description("Directory to search")]
        base_path: String,
        #[description("Target PDF filename to analyze")]
        target_filename: String,
        #[description("Minimum occurrences for a pattern to be significant (default: 2)")]
        min_pattern_occurrences: Option<u32>,
    ) -> McpResult<String> {
        let min_occ = min_pattern_occurrences.unwrap_or(2);

        let results = native::search::find_related_pdfs(&base_path, &target_filename, min_occ)
            .await
            .map_err(|e| map_tool_err("find_related_pdfs", e))?;

        Ok(format_search_results(&results))
    }

    // ── Stirling Bridge Tools (available with stirling-bridge feature) ──

    /// Perform OCR on a scanned PDF to make it searchable.
    /// Requires a Stirling PDF server (set STIRLING_PDF_URL).
    #[tool]
    async fn ocr_pdf(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Comma-separated language codes (e.g. 'eng', 'eng,spa')")]
        languages: Option<String>,
        #[description("Deskew pages before OCR")]
        deskew: Option<bool>,
        #[description("Clean pages before OCR")]
        clean: Option<bool>,
        #[description("Clean final output")]
        clean_final: Option<bool>,
        #[description("OCR processing mode: skip-text, force-ocr, or default")]
        ocr_type: Option<String>,
    ) -> McpResult<String> {
        ocr_pdf_impl(pdf_file, languages, deskew, clean, clean_final, ocr_type).await
    }

    /// Add a text watermark to a PDF document.
    /// Requires a Stirling PDF server (set STIRLING_PDF_URL).
    #[tool]
    async fn add_watermark(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Watermark text")]
        watermark_text: String,
        #[description("Font size (default: 30)")]
        font_size: Option<u32>,
        #[description("Opacity 0.0-1.0 (default: 0.5)")]
        opacity: Option<f32>,
        #[description("Rotation angle in degrees (default: 45)")]
        rotation: Option<i32>,
    ) -> McpResult<String> {
        add_watermark_impl(pdf_file, watermark_text, font_size, opacity, rotation).await
    }

    /// Convert PDF pages to image files.
    /// Requires a Stirling PDF server (set STIRLING_PDF_URL).
    #[tool]
    async fn convert_pdf_to_images(
        &self,
        #[description("PDF file as base64 data URL or file path")]
        pdf_file: String,
        #[description("Output image format: png, jpg, or gif")]
        image_format: Option<String>,
        #[description("Output DPI (default: 300)")]
        dpi: Option<u32>,
    ) -> McpResult<String> {
        convert_pdf_to_images_impl(pdf_file, image_format, dpi).await
    }

    /// Convert one or more images to a PDF document.
    /// Requires a Stirling PDF server (set STIRLING_PDF_URL).
    #[tool]
    async fn convert_images_to_pdf(
        &self,
        #[description("Array of image files as base64 data URLs or file paths")]
        image_files: Vec<String>,
        #[description("How images fit on pages: fillPage, fitDocumentToImage, maintainAspectRatio")]
        fit_option: Option<String>,
        #[description("Color mode: color, greyscale, blackwhite")]
        color_type: Option<String>,
    ) -> McpResult<String> {
        convert_images_to_pdf_impl(image_files, fit_option, color_type).await
    }
}

// ── Stirling Bridge implementation functions ──

/// Map a PdfError to an McpError, preserving structured error types
/// where the MCP protocol has appropriate variants.
fn map_tool_err(tool: &str, e: PdfError) -> McpError {
    match &e {
        PdfError::InvalidParameter(_)
        | PdfError::FileNotFound(_)
        | PdfError::Input(_)
        | PdfError::UnsupportedFormat(_)
        | PdfError::NotPdf(_) => McpError::invalid_params(e.to_string()),
        PdfError::StirlingNotConfigured | PdfError::ToolUnavailable { .. } => {
            McpError::configuration(e.to_string())
        }
        _ => McpError::tool_execution_failed(tool, e.to_string()),
    }
}

/// Parse a user-provided input string into a PdfInput, mapping errors.
fn parse_input(s: &str) -> std::result::Result<PdfInput, McpError> {
    PdfInput::from_user_string(s).map_err(|e| McpError::invalid_params(e.to_string()))
}

#[cfg(feature = "stirling-bridge")]
async fn ocr_pdf_impl(
    pdf_file: String,
    languages: Option<String>,
    deskew: Option<bool>,
    clean: Option<bool>,
    clean_final: Option<bool>,
    ocr_type: Option<String>,
) -> McpResult<String> {
    let config = stirling::StirlingConfig::from_env()
        .ok_or_else(|| McpError::configuration("Stirling PDF not configured. Set STIRLING_PDF_URL."))?;
    let input = parse_input(&pdf_file)?;
    let result = stirling::ocr::ocr_pdf(
        &config, input, languages.as_deref(),
        deskew, clean, clean_final, ocr_type.as_deref(),
    )
    .await
    .map_err(|e| map_tool_err("ocr_pdf", e))?;
    Ok(format_pdf_response(result, "OCR completed"))
}

#[cfg(not(feature = "stirling-bridge"))]
async fn ocr_pdf_impl(
    _pdf_file: String,
    _languages: Option<String>,
    _deskew: Option<bool>,
    _clean: Option<bool>,
    _clean_final: Option<bool>,
    _ocr_type: Option<String>,
) -> McpResult<String> {
    Err(McpError::configuration("OCR requires the stirling-bridge feature. Build with `--features stirling-bridge`."))
}

#[cfg(feature = "stirling-bridge")]
async fn add_watermark_impl(
    pdf_file: String,
    watermark_text: String,
    font_size: Option<u32>,
    opacity: Option<f32>,
    rotation: Option<i32>,
) -> McpResult<String> {
    let config = stirling::StirlingConfig::from_env()
        .ok_or_else(|| McpError::configuration("Stirling PDF not configured. Set STIRLING_PDF_URL."))?;
    let input = parse_input(&pdf_file)?;
    let result = stirling::watermark::add_watermark(
        &config, input, &watermark_text, font_size, opacity, rotation,
    )
    .await
    .map_err(|e| map_tool_err("add_watermark", e))?;
    Ok(format_pdf_response(result, "Watermark added"))
}

#[cfg(not(feature = "stirling-bridge"))]
async fn add_watermark_impl(
    _pdf_file: String,
    _watermark_text: String,
    _font_size: Option<u32>,
    _opacity: Option<f32>,
    _rotation: Option<i32>,
) -> McpResult<String> {
    Err(McpError::configuration("Watermark requires the stirling-bridge feature. Build with `--features stirling-bridge`."))
}

#[cfg(feature = "stirling-bridge")]
async fn convert_pdf_to_images_impl(
    pdf_file: String,
    image_format: Option<String>,
    dpi: Option<u32>,
) -> McpResult<String> {
    let config = stirling::StirlingConfig::from_env()
        .ok_or_else(|| McpError::configuration("Stirling PDF not configured. Set STIRLING_PDF_URL."))?;
    let input = parse_input(&pdf_file)?;
    let result = stirling::convert::pdf_to_images(&config, input, image_format.as_deref(), dpi)
        .await
        .map_err(|e| map_tool_err("convert_pdf_to_images", e))?;
    let fmt = image_format.as_deref().unwrap_or("png");
    let mime = match fmt {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        _ => "image/png",
    };
    let data_url = PdfOutput::data_url(result, mime);
    Ok(format!("Converted PDF to {fmt} images:\n\n{}", data_url.to_mcp_response()))
}

#[cfg(not(feature = "stirling-bridge"))]
async fn convert_pdf_to_images_impl(
    _pdf_file: String,
    _image_format: Option<String>,
    _dpi: Option<u32>,
) -> McpResult<String> {
    Err(McpError::configuration("PDF-to-images conversion requires the stirling-bridge feature. Build with `--features stirling-bridge`."))
}

#[cfg(feature = "stirling-bridge")]
async fn convert_images_to_pdf_impl(
    image_files: Vec<String>,
    fit_option: Option<String>,
    color_type: Option<String>,
) -> McpResult<String> {
    let config = stirling::StirlingConfig::from_env()
        .ok_or_else(|| McpError::configuration("Stirling PDF not configured. Set STIRLING_PDF_URL."))?;
    let inputs: Vec<PdfInput> = image_files
        .iter()
        .map(|s| parse_input(s))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let result = stirling::convert::images_to_pdf(
        &config, inputs, fit_option.as_deref(), color_type.as_deref(),
    )
    .await
    .map_err(|e| map_tool_err("convert_images_to_pdf", e))?;
    Ok(format_pdf_response(result, "Images converted to PDF"))
}

#[cfg(not(feature = "stirling-bridge"))]
async fn convert_images_to_pdf_impl(
    _image_files: Vec<String>,
    _fit_option: Option<String>,
    _color_type: Option<String>,
) -> McpResult<String> {
    Err(McpError::configuration("Images-to-PDF conversion requires the stirling-bridge feature. Build with `--features stirling-bridge`."))
}

