use mcp_pdf_core::{PdfInput, PdfResult};
use crate::client::send_pdf;
use crate::config::StirlingConfig;

pub async fn add_watermark(
    config: &StirlingConfig,
    input: PdfInput,
    watermark_text: &str,
    font_size: Option<u32>,
    opacity: Option<f32>,
    rotation: Option<i32>,
) -> PdfResult<Vec<u8>> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    fields.push(("watermarkType", "text".to_string()));
    fields.push(("watermarkText", watermark_text.to_string()));
    fields.push(("fontSize", font_size.unwrap_or(30).to_string()));
    fields.push(("opacity", opacity.unwrap_or(0.5).to_string()));
    fields.push(("rotation", rotation.unwrap_or(45).to_string()));

    send_pdf(config, "/api/v1/stamp/add-watermark-text", input, fields).await
}
