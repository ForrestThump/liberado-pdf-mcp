use mcp_pdf_core::{PdfInput, PdfResult};
use crate::client::send_pdf;
use crate::config::StirlingConfig;

pub async fn ocr_pdf(
    config: &StirlingConfig,
    input: PdfInput,
    languages: Option<&str>,
    deskew: Option<bool>,
    clean: Option<bool>,
    clean_final: Option<bool>,
    ocr_type: Option<&str>,
) -> PdfResult<Vec<u8>> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    fields.push(("languages", languages.unwrap_or("eng").to_string()));
    if let Some(v) = deskew { fields.push(("deskew", v.to_string())); }
    if let Some(v) = clean { fields.push(("clean", v.to_string())); }
    if let Some(v) = clean_final { fields.push(("cleanFinal", v.to_string())); }
    if let Some(v) = ocr_type { fields.push(("ocrType", v.to_string())); }

    send_pdf(config, "/api/v1/misc/ocr-pdf", input, fields).await
}
