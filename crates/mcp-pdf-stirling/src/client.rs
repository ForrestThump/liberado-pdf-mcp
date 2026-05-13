use mcp_pdf_core::{PdfError, PdfInput, PdfResult};
use reqwest::multipart;
use crate::config::StirlingConfig;

/// Common request helper: sends a multipart POST to a Stirling endpoint and
/// returns the response bytes. Handles headers, timeout, status checking, etc.
async fn send_request(
    config: &StirlingConfig,
    endpoint: &str,
    form: multipart::Form,
) -> PdfResult<Vec<u8>> {
    let url = config.api_url(endpoint);
    let mut request = config
        .client
        .post(&url)
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .multipart(form);

    if let Some(ref key) = config.api_key {
        request = request.header("X-API-KEY", key);
    }

    let response = request.send().await.map_err(|e| {
        PdfError::Other(format!("Stirling request failed: {e}"))
    })?;

    let status = response.status().as_u16();
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_else(|e| format!("[failed to read error body: {e}]"));
        return Err(PdfError::StirlingApi { status, message: body });
    }

    response.bytes().await.map(|b| b.to_vec()).map_err(|e| {
        PdfError::Other(format!("Failed to read Stirling response: {e}"))
    })
}

/// Send a PDF to a Stirling endpoint and return the response bytes.
pub async fn send_pdf(
    config: &StirlingConfig,
    endpoint: &str,
    input: PdfInput,
    extra_fields: Vec<(&str, String)>,
) -> PdfResult<Vec<u8>> {
    let bytes = input.into_bytes().await?;

    let part = multipart::Part::bytes(bytes)
        .file_name("document.pdf")
        .mime_str("application/pdf")
        .map_err(|e| PdfError::Input(format!("Failed to create multipart part: {e}")))?;

    let mut form = multipart::Form::new().part("fileInput", part);

    for (key, value) in &extra_fields {
        form = form.text(key.to_string(), value.clone());
    }

    send_request(config, endpoint, form).await
}

/// Send multiple files to a Stirling endpoint (for image-to-PDF).
pub async fn send_multiple_files(
    config: &StirlingConfig,
    endpoint: &str,
    inputs: Vec<PdfInput>,
    extra_fields: Vec<(&str, String)>,
) -> PdfResult<Vec<u8>> {
    let mut form = multipart::Form::new();

    for (i, input) in inputs.into_iter().enumerate() {
        let mime = input.mime_type().to_owned();
        let bytes = input.into_bytes().await?;
        let filename = format!("file_{}.{}", i, ext_from_mime(&mime));

        let part = multipart::Part::bytes(bytes)
            .file_name(filename)
            .mime_str(&mime)
            .map_err(|e| PdfError::Input(format!("Failed to create multipart part: {e}")))?;

        form = form.part("fileInput".to_string(), part);
    }

    for (key, value) in &extra_fields {
        form = form.text(key.to_string(), value.clone());
    }

    send_request(config, endpoint, form).await
}

fn ext_from_mime(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/tiff" => "tiff",
        _ => "png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ext_from_mime_known_types() {
        assert_eq!(ext_from_mime("image/png"), "png");
        assert_eq!(ext_from_mime("image/jpeg"), "jpg");
        assert_eq!(ext_from_mime("image/gif"), "gif");
        assert_eq!(ext_from_mime("image/tiff"), "tiff");
    }

    #[test]
    fn test_ext_from_mime_unknown_type() {
        assert_eq!(ext_from_mime("image/webp"), "png");
        assert_eq!(ext_from_mime("application/pdf"), "png");
        assert_eq!(ext_from_mime("text/plain"), "png");
    }
}
