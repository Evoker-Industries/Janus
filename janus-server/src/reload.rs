//! Configuration live reload using file watcher

use crate::AppState;
use anyhow::Result;
use janus_common::JanusConfig;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Watch configuration file for changes and reload automatically
pub async fn watch_config(state: Arc<AppState>) -> Result<()> {
    let config_path = state.config_path.clone();
    
    if !config_path.exists() {
        warn!("Config file does not exist, skipping file watcher");
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel(100);

    // Create watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    let _ = tx.blocking_send(());
                }
            }
        },
        Config::default(),
    )?;

    // Watch the config file's parent directory
    let watch_path = config_path.parent().unwrap_or(&config_path);
    watcher.watch(watch_path.as_ref(), RecursiveMode::NonRecursive)?;

    info!("Watching {} for changes", config_path.display());

    // Debounce timer
    let mut last_reload = std::time::Instant::now();
    let debounce_duration = std::time::Duration::from_millis(500);

    loop {
        if rx.recv().await.is_some() {
            // Debounce multiple rapid events
            let now = std::time::Instant::now();
            if now.duration_since(last_reload) < debounce_duration {
                continue;
            }
            last_reload = now;

            // Small delay to ensure file write is complete
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Reload configuration
            match reload_config(&state).await {
                Ok(()) => info!("Configuration reloaded successfully"),
                Err(e) => error!("Failed to reload configuration: {}", e),
            }
        }
    }
}

/// Reload configuration from file
pub async fn reload_config(state: &Arc<AppState>) -> Result<()> {
    let new_config = JanusConfig::load(&state.config_path)?;
    
    // Validate the new configuration
    validate_config(&new_config)?;
    
    // Update the configuration
    let mut config = state.config.write().await;
    *config = new_config;
    
    Ok(())
}

/// Validate configuration
fn validate_config(config: &JanusConfig) -> Result<()> {
    // Validate port numbers
    if config.server.port == 0 {
        anyhow::bail!("Server port cannot be 0");
    }
    
    if config.management.enabled && config.management.port == 0 {
        anyhow::bail!("Management port cannot be 0");
    }
    
    // Validate routes reference existing upstreams
    for route in &config.routes {
        if !config.upstreams.contains_key(&route.upstream) {
            anyhow::bail!("Route '{}' references non-existent upstream '{}'", route.path, route.upstream);
        }
    }
    
    // Validate upstreams have at least one server
    for (name, upstream) in &config.upstreams {
        if upstream.servers.is_empty() {
            anyhow::bail!("Upstream '{}' has no servers configured", name);
        }
    }
    
    Ok(())
}
