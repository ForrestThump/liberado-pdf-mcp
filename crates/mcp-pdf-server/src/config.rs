use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Native,
    Stirling,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TransportConfig {
    #[default]
    Stdio,
    Http { host: String, port: u16 },
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub engine: Engine,
    pub timeout_seconds: u64,
    pub transport: TransportConfig,
}

fn default_http_port() -> u16 {
    8080
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            engine: Engine::Native,
            timeout_seconds: 120,
            transport: TransportConfig::Stdio,
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = std::env::var("MCP_PDF_ENGINE") {
            config.engine = serde_json::from_str(&format!("\"{}\"", val.to_lowercase()))
                .unwrap_or(Engine::Native);
        }
        if let Ok(val) = std::env::var("MCP_PDF_TRANSPORT") {
            match val.to_lowercase().as_str() {
                "http" => {
                    let host = std::env::var("MCP_PDF_HTTP_HOST")
                        .unwrap_or_else(|_| "0.0.0.0".to_string());
                    let port = std::env::var("MCP_PDF_HTTP_PORT")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or_else(default_http_port);
                    config.transport = TransportConfig::Http { host, port };
                }
                _ => {
                    config.transport = TransportConfig::Stdio;
                }
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_default_transport_is_stdio() {
        let config = ServerConfig::default();
        assert_eq!(config.transport, TransportConfig::Stdio);
    }

    #[test]
    #[serial]
    fn test_from_env_transport_stdio() {
        unsafe { std::env::remove_var("MCP_PDF_TRANSPORT") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_HOST") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_PORT") };
        let config = ServerConfig::from_env();
        assert_eq!(config.transport, TransportConfig::Stdio);
    }

    #[test]
    #[serial]
    fn test_from_env_transport_http_defaults() {
        unsafe { std::env::set_var("MCP_PDF_TRANSPORT", "http") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_HOST") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_PORT") };
        let config = ServerConfig::from_env();
        assert_eq!(
            config.transport,
            TransportConfig::Http {
                host: "0.0.0.0".to_string(),
                port: 8080,
            }
        );
    }

    #[test]
    #[serial]
    fn test_from_env_transport_http_custom_host_port() {
        unsafe { std::env::set_var("MCP_PDF_TRANSPORT", "http") };
        unsafe { std::env::set_var("MCP_PDF_HTTP_HOST", "127.0.0.1") };
        unsafe { std::env::set_var("MCP_PDF_HTTP_PORT", "3000") };
        let config = ServerConfig::from_env();
        assert_eq!(
            config.transport,
            TransportConfig::Http {
                host: "127.0.0.1".to_string(),
                port: 3000,
            }
        );
    }

    #[test]
    #[serial]
    fn test_from_env_transport_invalid_value_falls_back_to_stdio() {
        unsafe { std::env::set_var("MCP_PDF_TRANSPORT", "tcp") };
        let config = ServerConfig::from_env();
        assert_eq!(config.transport, TransportConfig::Stdio);
    }

    #[test]
    #[serial]
    fn test_from_env_transport_http_case_insensitive() {
        unsafe { std::env::set_var("MCP_PDF_TRANSPORT", "HTTP") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_HOST") };
        unsafe { std::env::remove_var("MCP_PDF_HTTP_PORT") };
        let config = ServerConfig::from_env();
        assert_eq!(
            config.transport,
            TransportConfig::Http {
                host: "0.0.0.0".to_string(),
                port: 8080,
            }
        );
    }
}
