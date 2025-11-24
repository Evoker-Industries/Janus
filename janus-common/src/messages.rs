//! IPC messages between server and TUI

use serde::{Deserialize, Serialize};
use crate::config::JanusConfig;

/// Messages sent from TUI to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    /// Request current server status
    GetStatus,
    
    /// Request current configuration
    GetConfig,
    
    /// Update configuration (triggers live reload)
    UpdateConfig(JanusConfig),
    
    /// Add a new route
    AddRoute(crate::config::RouteConfig),
    
    /// Remove a route by path
    RemoveRoute(String),
    
    /// Add or update an upstream
    UpdateUpstream {
        name: String,
        config: crate::config::UpstreamConfig,
    },
    
    /// Remove an upstream
    RemoveUpstream(String),
    
    /// Reload configuration from file
    ReloadConfig,
    
    /// Get server statistics
    GetStats,
    
    /// Gracefully shutdown the server
    Shutdown,
}

/// Messages sent from server to TUI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    /// Server status response
    Status(ServerStatus),
    
    /// Current configuration
    Config(JanusConfig),
    
    /// Server statistics
    Stats(ServerStats),
    
    /// Operation success
    Success(String),
    
    /// Operation error
    Error(String),
    
    /// Configuration was reloaded (broadcast to all clients)
    ConfigReloaded,
    
    /// Server is shutting down
    ShuttingDown,
}

/// Server status information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerStatus {
    /// Server is running
    pub running: bool,
    
    /// Uptime in seconds
    pub uptime_secs: u64,
    
    /// Number of active connections
    pub active_connections: u64,
    
    /// Number of configured routes
    pub route_count: usize,
    
    /// Number of configured upstreams
    pub upstream_count: usize,
    
    /// Server version
    pub version: String,
    
    /// Listening address
    pub listen_address: String,
}

/// Server statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerStats {
    /// Total requests handled
    pub total_requests: u64,
    
    /// Total bytes received
    pub bytes_received: u64,
    
    /// Total bytes sent
    pub bytes_sent: u64,
    
    /// Requests per second (average)
    pub requests_per_second: f64,
    
    /// Response status code counts
    pub status_codes: StatusCodeStats,
    
    /// Upstream statistics
    pub upstream_stats: std::collections::HashMap<String, UpstreamStats>,
}

/// HTTP status code statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusCodeStats {
    /// 2xx responses
    pub success: u64,
    /// 3xx responses
    pub redirect: u64,
    /// 4xx responses
    pub client_error: u64,
    /// 5xx responses
    pub server_error: u64,
}

/// Per-upstream statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpstreamStats {
    /// Total requests to this upstream
    pub requests: u64,
    
    /// Failed requests
    pub failures: u64,
    
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    
    /// Backend server health status
    pub healthy_servers: usize,
    
    /// Total backend servers
    pub total_servers: usize,
}
