use mcp_pdf_server::{PdfServer, ServerConfig, TransportConfig};
use turbomcp::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ServerConfig::from_env();
    let transport_cfg = config.transport.clone();

    let builder = PdfServer { config }.builder();

    let builder = match transport_cfg {
        TransportConfig::Stdio => {
            builder.transport(turbomcp::Transport::stdio())
        }
        TransportConfig::Http { host, port } => {
            let addr = format!("{host}:{port}");
            #[cfg(feature = "http")]
            {
                builder.transport(turbomcp::Transport::http(addr))
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = addr;
                eprintln!("Error: HTTP transport is not available in this build.");
                eprintln!("Rebuild with: cargo build --features http");
                std::process::exit(1);
            }
        }
    };

    builder.serve().await.unwrap();
}
