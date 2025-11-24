//! Janus Server - Web server and reverse proxy with live reloading

mod proxy;
mod server;
mod management;
mod reload;
mod stats;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use janus_common::JanusConfig;

/// Shared application state
pub struct AppState {
    pub config: Arc<RwLock<JanusConfig>>,
    pub stats: Arc<RwLock<stats::Stats>>,
    pub start_time: std::time::Instant,
    pub config_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("janus=info".parse()?)
        )
        .init();

    info!("Starting Janus Server v{}", env!("CARGO_PKG_VERSION"));

    // Determine config path
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("janus.toml"));

    // Load or create default configuration
    let config = if config_path.exists() {
        info!("Loading configuration from {}", config_path.display());
        JanusConfig::load(&config_path)?
    } else {
        info!("No configuration file found, using defaults");
        let config = JanusConfig::default();
        // Save default config for reference
        if let Err(e) = config.save(&config_path) {
            error!("Failed to save default config: {}", e);
        }
        config
    };

    // Create shared state
    let state = Arc::new(AppState {
        config: Arc::new(RwLock::new(config.clone())),
        stats: Arc::new(RwLock::new(stats::Stats::default())),
        start_time: std::time::Instant::now(),
        config_path: config_path.clone(),
    });

    // Start file watcher for live reloading
    let reload_state = state.clone();
    let reload_handle = tokio::spawn(async move {
        if let Err(e) = reload::watch_config(reload_state).await {
            error!("Config watcher error: {}", e);
        }
    });

    // Start management WebSocket server
    let mgmt_state = state.clone();
    let mgmt_handle = if config.management.enabled {
        let addr = format!("{}:{}", config.management.address, config.management.port);
        info!("Starting management API on ws://{}", addr);
        Some(tokio::spawn(async move {
            if let Err(e) = management::run_management_server(mgmt_state).await {
                error!("Management server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Start HTTP server
    let server_state = state.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run_server(server_state).await {
            error!("HTTP server error: {}", e);
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        result = server_handle => {
            if let Err(e) = result {
                error!("Server task failed: {}", e);
            }
        }
    }

    // Cleanup
    reload_handle.abort();
    if let Some(handle) = mgmt_handle {
        handle.abort();
    }

    info!("Janus Server shutdown complete");
    Ok(())
}
