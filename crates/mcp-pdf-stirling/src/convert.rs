use mcp_pdf_core::{PdfInput, PdfResult};
use crate::client::{send_pdf, send_multiple_files};
use crate::config::StirlingConfig;

/// Convert PDF pages to images.
pub async fn pdf_to_images(
    config: &StirlingConfig,
    input: PdfInput,
    image_format: Option<&str>,
    dpi: Option<u32>,
) -> PdfResult<Vec<u8>> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    fields.push(("imageFormat", image_format.unwrap_or("png").to_string()));
    fields.push(("dpi", dpi.unwrap_or(300).to_string()));

    send_pdf(config, "/api/v1/convert/pdf/img", input, fields).await
}

/// Convert images to PDF.
pub async fn images_to_pdf(
    config: &StirlingConfig,
    inputs: Vec<PdfInput>,
    fit_option: Option<&str>,
    color_type: Option<&str>,
) -> PdfResult<Vec<u8>> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    fields.push(("fitOption", fit_option.unwrap_or("maintainAspectRatio").to_string()));
    fields.push(("colorType", color_type.unwrap_or("color").to_string()));

    send_multiple_files(config, "/api/v1/convert/img/pdf", inputs, fields).await
}
