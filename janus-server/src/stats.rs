//! Server statistics tracking

use janus_common::StatusCodeStats;
use serde::{Deserialize, Serialize};

/// Server statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    /// Total requests handled
    pub total_requests: u64,
    
    /// Total bytes received
    pub bytes_received: u64,
    
    /// Total bytes sent
    pub bytes_sent: u64,
    
    /// Response status code counts
    pub status_codes: StatusCodeStats,
}
