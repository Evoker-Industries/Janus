//! Application state and logic

use crate::client::ManagementClient;
use crossterm::event::{KeyCode, KeyEvent};
use janus_common::{ClientMessage, JanusConfig, ServerMessage, ServerStats, ServerStatus};
use std::time::{Duration, Instant};
use tracing::{debug, error};

/// Application state
pub struct App {
    /// Server address
    pub server_addr: String,
    
    /// WebSocket client
    pub client: Option<ManagementClient>,
    
    /// Connection status
    pub connected: bool,
    
    /// Current tab
    pub current_tab: Tab,
    
    /// Server status
    pub status: Option<ServerStatus>,
    
    /// Server configuration
    pub config: Option<JanusConfig>,
    
    /// Server statistics
    pub stats: Option<ServerStats>,
    
    /// Status messages
    pub messages: Vec<StatusMessage>,
    
    /// Selected item in lists
    pub selected_route: usize,
    pub selected_upstream: usize,
    
    /// Edit mode
    pub edit_mode: EditMode,
    
    /// Input buffer for editing
    pub input_buffer: String,
    
    /// Last refresh time
    pub last_refresh: Instant,
    
    /// Refresh interval
    pub refresh_interval: Duration,
}

/// Available tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Status,
    Routes,
    Upstreams,
    Config,
    Stats,
    Help,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[Tab::Status, Tab::Routes, Tab::Upstreams, Tab::Config, Tab::Stats, Tab::Help]
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            Tab::Status => "Status",
            Tab::Routes => "Routes",
            Tab::Upstreams => "Upstreams",
            Tab::Config => "Config",
            Tab::Stats => "Stats",
            Tab::Help => "Help",
        }
    }
}

/// Edit mode state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditMode {
    None,
    AddRoute,
    EditRoute,
    AddUpstream,
    EditUpstream,
}

/// Status message for display
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub timestamp: Instant,
}

impl App {
    pub fn new(server_addr: String) -> Self {
        Self {
            server_addr,
            client: None,
            connected: false,
            current_tab: Tab::Status,
            status: None,
            config: None,
            stats: None,
            messages: Vec::new(),
            selected_route: 0,
            selected_upstream: 0,
            edit_mode: EditMode::None,
            input_buffer: String::new(),
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(2),
        }
    }

    /// Connect to the server
    pub async fn connect(&mut self) {
        let addr = format!("ws://{}", self.server_addr);
        match ManagementClient::connect(&addr).await {
            Ok(client) => {
                self.client = Some(client);
                self.connected = true;
                self.add_message("Connected to server", false);
                
                // Request initial data
                self.send_message(ClientMessage::GetStatus).await;
                self.send_message(ClientMessage::GetConfig).await;
                self.send_message(ClientMessage::GetStats).await;
            }
            Err(e) => {
                self.connected = false;
                self.add_message(&format!("Connection failed: {}", e), true);
            }
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) {
        self.client = None;
        self.connected = false;
        self.add_message("Disconnected", false);
    }

    /// Send a message to the server
    pub async fn send_message(&mut self, msg: ClientMessage) {
        if let Some(ref mut client) = self.client {
            if let Err(e) = client.send(msg).await {
                error!("Failed to send message: {}", e);
                self.add_message(&format!("Send failed: {}", e), true);
            }
        }
    }

    /// Process incoming messages from server
    pub async fn process_messages(&mut self) {
        // Collect messages first to avoid borrow issues
        let messages: Vec<ServerMessage> = if let Some(ref mut client) = self.client {
            let mut msgs = Vec::new();
            while let Some(msg) = client.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };
        
        // Then handle each message
        for msg in messages {
            self.handle_server_message(msg);
        }
    }

    /// Handle a message from the server
    fn handle_server_message(&mut self, msg: ServerMessage) {
        debug!("Received: {:?}", msg);
        match msg {
            ServerMessage::Status(status) => {
                self.status = Some(status);
            }
            ServerMessage::Config(config) => {
                self.config = Some(config);
            }
            ServerMessage::Stats(stats) => {
                self.stats = Some(stats);
            }
            ServerMessage::Success(msg) => {
                self.add_message(&msg, false);
            }
            ServerMessage::Error(msg) => {
                self.add_message(&msg, true);
            }
            ServerMessage::ConfigReloaded => {
                self.add_message("Configuration reloaded", false);
                // Request updated config
                if let Some(ref mut client) = self.client {
                    let _ = futures::executor::block_on(client.send(ClientMessage::GetConfig));
                }
            }
            ServerMessage::ShuttingDown => {
                self.add_message("Server is shutting down", true);
                self.connected = false;
            }
        }
    }

    /// Auto-refresh data from server
    pub async fn auto_refresh(&mut self) {
        if self.connected && self.last_refresh.elapsed() >= self.refresh_interval {
            self.send_message(ClientMessage::GetStatus).await;
            self.send_message(ClientMessage::GetStats).await;
            self.last_refresh = Instant::now();
        }
    }

    /// Add a status message
    pub fn add_message(&mut self, text: &str, is_error: bool) {
        self.messages.push(StatusMessage {
            text: text.to_string(),
            is_error,
            timestamp: Instant::now(),
        });
        
        // Keep only last 10 messages
        if self.messages.len() > 10 {
            self.messages.remove(0);
        }
    }

    /// Check if in editing mode
    pub fn is_editing(&self) -> bool {
        self.edit_mode != EditMode::None
    }

    /// Handle key input
    pub async fn handle_key(&mut self, key: KeyEvent) {
        // Handle editing mode
        if self.is_editing() {
            match key.code {
                KeyCode::Esc => {
                    self.edit_mode = EditMode::None;
                    self.input_buffer.clear();
                }
                KeyCode::Enter => {
                    self.submit_edit().await;
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                _ => {}
            }
            return;
        }

        // Normal mode key handling
        match key.code {
            // Tab navigation
            KeyCode::Tab => {
                let tabs = Tab::all();
                let current_idx = tabs.iter().position(|&t| t == self.current_tab).unwrap_or(0);
                self.current_tab = tabs[(current_idx + 1) % tabs.len()];
            }
            KeyCode::BackTab => {
                let tabs = Tab::all();
                let current_idx = tabs.iter().position(|&t| t == self.current_tab).unwrap_or(0);
                self.current_tab = tabs[(current_idx + tabs.len() - 1) % tabs.len()];
            }
            
            // Number keys for direct tab selection
            KeyCode::Char('1') => self.current_tab = Tab::Status,
            KeyCode::Char('2') => self.current_tab = Tab::Routes,
            KeyCode::Char('3') => self.current_tab = Tab::Upstreams,
            KeyCode::Char('4') => self.current_tab = Tab::Config,
            KeyCode::Char('5') => self.current_tab = Tab::Stats,
            KeyCode::Char('6') => self.current_tab = Tab::Help,
            
            // Refresh
            KeyCode::Char('r') => {
                if self.connected {
                    self.send_message(ClientMessage::GetStatus).await;
                    self.send_message(ClientMessage::GetConfig).await;
                    self.send_message(ClientMessage::GetStats).await;
                    self.add_message("Refreshing...", false);
                }
            }
            
            // Reconnect
            KeyCode::Char('c') => {
                if !self.connected {
                    self.connect().await;
                }
            }
            
            // Reload config
            KeyCode::Char('R') => {
                if self.connected {
                    self.send_message(ClientMessage::ReloadConfig).await;
                }
            }
            
            // List navigation
            KeyCode::Up | KeyCode::Char('k') => {
                match self.current_tab {
                    Tab::Routes => {
                        if self.selected_route > 0 {
                            self.selected_route -= 1;
                        }
                    }
                    Tab::Upstreams => {
                        if self.selected_upstream > 0 {
                            self.selected_upstream -= 1;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.current_tab {
                    Tab::Routes => {
                        if let Some(ref config) = self.config {
                            if self.selected_route < config.routes.len().saturating_sub(1) {
                                self.selected_route += 1;
                            }
                        }
                    }
                    Tab::Upstreams => {
                        if let Some(ref config) = self.config {
                            if self.selected_upstream < config.upstreams.len().saturating_sub(1) {
                                self.selected_upstream += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            
            // Delete selected item
            KeyCode::Char('d') | KeyCode::Delete => {
                match self.current_tab {
                    Tab::Routes => {
                        if let Some(ref config) = self.config {
                            if let Some(route) = config.routes.get(self.selected_route) {
                                let path = route.path.clone();
                                self.send_message(ClientMessage::RemoveRoute(path)).await;
                                self.send_message(ClientMessage::GetConfig).await;
                            }
                        }
                    }
                    Tab::Upstreams => {
                        if let Some(ref config) = self.config {
                            let names: Vec<_> = config.upstreams.keys().collect();
                            if let Some(name) = names.get(self.selected_upstream) {
                                self.send_message(ClientMessage::RemoveUpstream((*name).clone())).await;
                                self.send_message(ClientMessage::GetConfig).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
            
            _ => {}
        }
    }

    /// Submit the current edit
    async fn submit_edit(&mut self) {
        // For now, just clear the edit mode
        // Full implementation would parse input and send appropriate commands
        self.edit_mode = EditMode::None;
        self.input_buffer.clear();
    }
}
