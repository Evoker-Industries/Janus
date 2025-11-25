//! Management WebSocket server for TUI connections

use crate::AppState;
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use janus_common::{ClientMessage, JanusConfig, ServerMessage, ServerStats, ServerStatus};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

/// Run the management WebSocket server
pub async fn run_management_server(state: Arc<AppState>) -> Result<()> {
    let config = state.config.read().await;
    let addr = format!("{}:{}", config.management.address, config.management.port);
    drop(config);

    let listener = TcpListener::bind(&addr).await?;
    info!("Management server listening on ws://{}", addr);

    while let Ok((stream, peer_addr)) = listener.accept().await {
        let state = state.clone();

        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    info!("New management connection from {}", peer_addr);
                    if let Err(e) = handle_connection(ws_stream, state).await {
                        error!("Connection error: {}", e);
                    }
                    info!("Management connection from {} closed", peer_addr);
                }
                Err(e) => {
                    error!("WebSocket handshake failed: {}", e);
                }
            }
        });
    }

    Ok(())
}

/// Handle a single WebSocket connection
async fn handle_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
) -> Result<()> {
    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(client_msg) => {
                    let response = handle_message(client_msg, &state).await;
                    let response_text = serde_json::to_string(&response)?;
                    write.send(Message::Text(response_text)).await?;
                }
                Err(e) => {
                    warn!("Invalid message format: {}", e);
                    let error = ServerMessage::Error(format!("Invalid message: {}", e));
                    let response_text = serde_json::to_string(&error)?;
                    write.send(Message::Text(response_text)).await?;
                }
            },
            Ok(Message::Close(_)) => {
                debug!("Client initiated close");
                break;
            }
            Ok(Message::Ping(data)) => {
                write.send(Message::Pong(data)).await?;
            }
            Ok(_) => {
                // Ignore other message types
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Handle a client message and return a response
async fn handle_message(msg: ClientMessage, state: &Arc<AppState>) -> ServerMessage {
    match msg {
        ClientMessage::GetStatus => {
            let config = state.config.read().await;
            let stats = state.stats.read().await;

            ServerMessage::Status(ServerStatus {
                running: true,
                uptime_secs: state.start_time.elapsed().as_secs(),
                active_connections: stats.total_requests, // Simplified
                route_count: config.routes.len(),
                upstream_count: config.upstreams.len(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                listen_address: format!("{}:{}", config.server.bind_address, config.server.port),
            })
        }

        ClientMessage::GetConfig => {
            let config = state.config.read().await;
            ServerMessage::Config(config.clone())
        }

        ClientMessage::UpdateConfig(new_config) => {
            // Validate and update configuration
            match validate_and_update_config(state, new_config).await {
                Ok(()) => {
                    // Save to file
                    let config = state.config.read().await;
                    if let Err(e) = config.save(&state.config_path) {
                        return ServerMessage::Error(format!("Failed to save config: {}", e));
                    }
                    ServerMessage::Success("Configuration updated".to_string())
                }
                Err(e) => ServerMessage::Error(e.to_string()),
            }
        }

        ClientMessage::AddRoute(route) => {
            let mut config = state.config.write().await;

            // Check if upstream exists
            if !config.upstreams.contains_key(&route.upstream) {
                return ServerMessage::Error(format!("Upstream '{}' not found", route.upstream));
            }

            // Check for duplicate route
            if config.routes.iter().any(|r| r.path == route.path) {
                return ServerMessage::Error(format!("Route '{}' already exists", route.path));
            }

            config.routes.push(route);

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success("Route added".to_string())
        }

        ClientMessage::RemoveRoute(path) => {
            let mut config = state.config.write().await;
            let initial_len = config.routes.len();
            config.routes.retain(|r| r.path != path);

            if config.routes.len() == initial_len {
                return ServerMessage::Error(format!("Route '{}' not found", path));
            }

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success("Route removed".to_string())
        }

        ClientMessage::UpdateUpstream {
            name,
            config: upstream_config,
        } => {
            let mut config = state.config.write().await;
            config.upstreams.insert(name.clone(), upstream_config);

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!("Upstream '{}' updated", name))
        }

        ClientMessage::RemoveUpstream(name) => {
            let mut config = state.config.write().await;

            // Check if any routes use this upstream
            if config.routes.iter().any(|r| r.upstream == name) {
                return ServerMessage::Error(format!(
                    "Cannot remove upstream '{}': still in use by routes",
                    name
                ));
            }

            if config.upstreams.remove(&name).is_none() {
                return ServerMessage::Error(format!("Upstream '{}' not found", name));
            }

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!("Upstream '{}' removed", name))
        }

        ClientMessage::UpdateServerPort(port) => {
            let mut config = state.config.write().await;
            let old_port = config.server.port;
            config.server.port = port;

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!(
                "Server port changed from {} to {}. Restart server to apply.",
                old_port, port
            ))
        }

        ClientMessage::UpdateBindAddress(address) => {
            let mut config = state.config.write().await;
            let old_address = config.server.bind_address.clone();
            config.server.bind_address = address.clone();

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!(
                "Bind address changed from {} to {}. Restart server to apply.",
                old_address, address
            ))
        }

        ClientMessage::AddStaticDir(static_config) => {
            let mut config = state.config.write().await;

            // Check for duplicate path
            if config
                .static_files
                .iter()
                .any(|s| s.path == static_config.path)
            {
                return ServerMessage::Error(format!(
                    "Static directory '{}' already exists",
                    static_config.path
                ));
            }

            config.static_files.push(static_config.clone());

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!("Static directory '{}' added", static_config.path))
        }

        ClientMessage::RemoveStaticDir(path) => {
            let mut config = state.config.write().await;
            let initial_len = config.static_files.len();
            config.static_files.retain(|s| s.path != path);

            if config.static_files.len() == initial_len {
                return ServerMessage::Error(format!("Static directory '{}' not found", path));
            }

            // Save to file
            if let Err(e) = config.save(&state.config_path) {
                return ServerMessage::Error(format!("Failed to save config: {}", e));
            }

            ServerMessage::Success(format!("Static directory '{}' removed", path))
        }

        ClientMessage::ReloadConfig => match crate::reload::reload_config(state).await {
            Ok(()) => ServerMessage::Success("Configuration reloaded from file".to_string()),
            Err(e) => ServerMessage::Error(format!("Failed to reload config: {}", e)),
        },

        ClientMessage::GetStats => {
            let stats = state.stats.read().await;
            let uptime = state.start_time.elapsed().as_secs_f64();

            ServerMessage::Stats(ServerStats {
                total_requests: stats.total_requests,
                bytes_received: stats.bytes_received,
                bytes_sent: stats.bytes_sent,
                requests_per_second: if uptime > 0.0 {
                    stats.total_requests as f64 / uptime
                } else {
                    0.0
                },
                status_codes: stats.status_codes.clone(),
                upstream_stats: std::collections::HashMap::new(),
            })
        }

        ClientMessage::Shutdown => {
            // In a real implementation, this would trigger graceful shutdown
            ServerMessage::ShuttingDown
        }
    }
}

/// Validate and update configuration
async fn validate_and_update_config(state: &Arc<AppState>, new_config: JanusConfig) -> Result<()> {
    // Validate routes reference existing upstreams
    for route in &new_config.routes {
        if !new_config.upstreams.contains_key(&route.upstream) {
            anyhow::bail!(
                "Route '{}' references non-existent upstream '{}'",
                route.path,
                route.upstream
            );
        }
    }

    // Update configuration
    let mut config = state.config.write().await;
    *config = new_config;

    Ok(())
}
