//! Configuration types for Janus server

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Main server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JanusConfig {
    /// Global server settings
    #[serde(default)]
    pub server: ServerConfig,
    
    /// Management API settings
    #[serde(default)]
    pub management: ManagementConfig,
    
    /// Upstream servers for reverse proxy
    #[serde(default)]
    pub upstreams: HashMap<String, UpstreamConfig>,
    
    /// Route definitions
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    
    /// Static file serving configuration
    #[serde(default)]
    pub static_files: Vec<StaticFileConfig>,
}

/// Server listening configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Address to bind to
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    
    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,
    
    /// Number of worker threads (0 = auto)
    #[serde(default)]
    pub workers: usize,
    
    /// Enable access logging
    #[serde(default = "default_true")]
    pub access_log: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            port: default_port(),
            workers: 0,
            access_log: true,
        }
    }
}

/// Management API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagementConfig {
    /// Enable management API
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// WebSocket address for management connections
    #[serde(default = "default_management_address")]
    pub address: String,
    
    /// Management port
    #[serde(default = "default_management_port")]
    pub port: u16,
}

impl Default for ManagementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            address: default_management_address(),
            port: default_management_port(),
        }
    }
}

/// Upstream server configuration for reverse proxy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// List of backend servers
    pub servers: Vec<BackendServer>,
    
    /// Load balancing strategy
    #[serde(default)]
    pub load_balancing: LoadBalancing,
    
    /// Health check configuration
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
}

/// Backend server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendServer {
    /// Server address (host:port or URL)
    pub address: String,
    
    /// Server weight for weighted load balancing
    #[serde(default = "default_weight")]
    pub weight: u32,
    
    /// Whether this server is a backup
    #[serde(default)]
    pub backup: bool,
}

/// Load balancing strategies for distributing requests across backend servers
/// 
/// # Examples
/// 
/// ```toml
/// [upstreams.backend]
/// servers = [
///     { address = "localhost:3001", weight = 1 },
///     { address = "localhost:3002", weight = 2 }
/// ]
/// load_balancing = "round_robin"  # or "least_connections", "random", "ip_hash"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalancing {
    /// Round-robin distribution - requests are distributed sequentially to each server
    #[default]
    RoundRobin,
    /// Least connections - requests go to the server with fewest active connections
    LeastConnections,
    /// Random selection - requests are randomly distributed to servers
    Random,
    /// IP hash for session persistence - the same client IP always goes to the same server
    /// This is useful for applications that require sticky sessions
    IpHash,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Interval between health checks in seconds
    #[serde(default = "default_health_interval")]
    pub interval: u64,
    
    /// Health check timeout in seconds
    #[serde(default = "default_health_timeout")]
    pub timeout: u64,
    
    /// Path to check
    #[serde(default = "default_health_path")]
    pub path: String,
}

/// Route configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    /// Route path pattern (supports wildcards)
    pub path: String,
    
    /// HTTP methods to match (empty = all)
    #[serde(default)]
    pub methods: Vec<String>,
    
    /// Upstream name to proxy to
    pub upstream: String,
    
    /// Path rewrite rules
    #[serde(default)]
    pub rewrite: Option<String>,
    
    /// Additional headers to add
    #[serde(default)]
    pub headers: HashMap<String, String>,
    
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

/// Static file serving configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticFileConfig {
    /// URL path prefix
    pub path: String,
    
    /// Root directory for static files
    pub root: String,
    
    /// Index file name
    #[serde(default = "default_index")]
    pub index: String,
    
    /// Enable directory listing
    #[serde(default)]
    pub directory_listing: bool,
}

// Default value functions
fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_management_address() -> String {
    "127.0.0.1".to_string()
}

fn default_management_port() -> u16 {
    9090
}

fn default_true() -> bool {
    true
}

fn default_weight() -> u32 {
    1
}

fn default_health_interval() -> u64 {
    30
}

fn default_health_timeout() -> u64 {
    5
}

fn default_health_path() -> String {
    "/health".to_string()
}

fn default_timeout() -> u64 {
    60
}

fn default_index() -> String {
    "index.html".to_string()
}

impl JanusConfig {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::IoError(e.to_string()))?;
        Self::from_toml(&content)
    }
    
    /// Parse configuration from TOML string
    pub fn from_toml(content: &str) -> Result<Self, ConfigError> {
        toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    /// Save configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;
        std::fs::write(path.as_ref(), content)
            .map_err(|e| ConfigError::IoError(e.to_string()))?;
        Ok(())
    }
    
    /// Convert to TOML string
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::SerializeError(e.to_string()))
    }
}

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Serialize error: {0}")]
    SerializeError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = JanusConfig::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.bind_address, "0.0.0.0");
        assert!(config.management.enabled);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[server]
bind_address = "127.0.0.1"
port = 3000

[management]
enabled = true
port = 9090

[upstreams.backend]
servers = [
    { address = "localhost:8001", weight = 1 },
    { address = "localhost:8002", weight = 2 }
]
load_balancing = "round_robin"

[[routes]]
path = "/api/*"
upstream = "backend"
timeout = 30

[[static_files]]
path = "/"
root = "/var/www/html"
index = "index.html"
"#;
        
        let config = JanusConfig::from_toml(toml).unwrap();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.bind_address, "127.0.0.1");
        assert_eq!(config.upstreams.get("backend").unwrap().servers.len(), 2);
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.static_files.len(), 1);
    }
}
