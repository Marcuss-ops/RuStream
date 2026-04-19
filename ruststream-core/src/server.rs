//! HTTP Server module (feature-gated).
//!
//! This module provides an optional HTTP API for the RustStream engine.
//! Currently a placeholder - full implementation pending.

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address.
    pub host: String,
    /// Port number.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
    }
}

/// Start the HTTP server (placeholder).
pub async fn start_server(config: ServerConfig) -> Result<(), String> {
    log::info!("Server module is not yet implemented");
    log::info!("Requested config: {}:{}", config.host, config.port);
    Err("HTTP server not implemented".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
    }
}
