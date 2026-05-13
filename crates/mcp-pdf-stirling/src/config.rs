#[derive(Debug, Clone)]
pub struct StirlingConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_secs: u64,
    pub client: reqwest::Client,
}

impl StirlingConfig {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("STIRLING_PDF_URL").ok()?;
        let api_key = std::env::var("STIRLING_PDF_API_KEY").ok();
        let timeout_secs = std::env::var("STIRLING_PDF_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120);
        Some(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            timeout_secs,
            client: reqwest::Client::new(),
        })
    }

    pub fn api_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url() {
        let config = StirlingConfig {
            base_url: "http://localhost:8080".to_string(),
            api_key: None,
            timeout_secs: 120,
            client: reqwest::Client::new(),
        };
        assert_eq!(config.api_url("/api/v1/test"), "http://localhost:8080/api/v1/test");
    }

    #[test]
    fn test_api_url_trailing_slash_in_base() {
        let config = StirlingConfig {
            base_url: "http://localhost:8080/".to_string(),
            api_key: None,
            timeout_secs: 120,
            client: reqwest::Client::new(),
        };
        // trailing slash stripped, path leading slash stripped, joined with /
        assert_eq!(config.api_url("/api/v1/test"), "http://localhost:8080/api/v1/test");
    }

    #[test]
    fn test_api_url_no_leading_slash_in_path() {
        let config = StirlingConfig {
            base_url: "http://localhost:8080".to_string(),
            api_key: None,
            timeout_secs: 120,
            client: reqwest::Client::new(),
        };
        assert_eq!(config.api_url("api/v1/test"), "http://localhost:8080/api/v1/test");
    }

    #[test]
    fn test_from_env_not_configured() {
        // When STIRLING_PDF_URL is not set, from_env returns None
        unsafe { std::env::remove_var("STIRLING_PDF_URL") };
        unsafe { std::env::remove_var("STIRLING_PDF_API_KEY") };
        assert!(StirlingConfig::from_env().is_none());
    }

    #[test]
    fn test_from_env_configured() {
        unsafe { std::env::set_var("STIRLING_PDF_URL", "http://localhost:9999/") };
        unsafe { std::env::remove_var("STIRLING_PDF_API_KEY") };
        let config = StirlingConfig::from_env().unwrap();
        assert_eq!(config.base_url, "http://localhost:9999");
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_from_env_with_api_key() {
        unsafe { std::env::set_var("STIRLING_PDF_URL", "http://localhost:9999") };
        unsafe { std::env::set_var("STIRLING_PDF_API_KEY", "secret123") };
        let config = StirlingConfig::from_env().unwrap();
        assert_eq!(config.api_key.unwrap(), "secret123");
    }
}
