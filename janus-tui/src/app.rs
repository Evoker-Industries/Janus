//! Application state and logic

use crate::client::ManagementClient;
use crossterm::event::{KeyCode, KeyEvent};
use janus_common::{ClientMessage, JanusConfig, ServerMessage, ServerStats, ServerStatus};
use janus_common::config::{RouteConfig, StaticFileConfig};
use std::collections::HashMap;
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
    pub selected_static_dir: usize,
    
    /// Edit mode
    pub edit_mode: EditMode,
    
    /// Input buffer for editing
    pub input_buffer: String,
    
    /// New route being created
    pub new_route: NewRoute,
    
    /// New static directory being created
    pub new_static_dir: NewStaticDir,
    
    /// Last refresh time
    pub last_refresh: Instant,
    
    /// Refresh interval
    pub refresh_interval: Duration,
    
    /// Flag to request config refresh
    needs_config_refresh: bool,
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
    /// Adding a new route - step 1: path
    AddRoutePath,
    /// Adding a new route - step 2: upstream
    AddRouteUpstream,
    /// Adding a new route - step 3: timeout
    AddRouteTimeout,
    /// Editing server port
    EditServerPort,
    /// Adding static directory - step 1: URL path
    AddStaticPath,
    /// Adding static directory - step 2: root directory
    AddStaticRoot,
}

/// New route being created
#[derive(Debug, Clone, Default)]
pub struct NewRoute {
    pub path: String,
    pub upstream: String,
    pub timeout: String,
}

/// New static directory being created
#[derive(Debug, Clone, Default)]
pub struct NewStaticDir {
    pub path: String,
    pub root: String,
}

/// Status message for display
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
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
            selected_static_dir: 0,
            edit_mode: EditMode::None,
            input_buffer: String::new(),
            new_route: NewRoute::default(),
            new_static_dir: NewStaticDir::default(),
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(2),
            needs_config_refresh: false,
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
                // Set flag to request updated config in next async tick
                self.needs_config_refresh = true;
            }
            ServerMessage::ShuttingDown => {
                self.add_message("Server is shutting down", true);
                self.connected = false;
            }
        }
    }

    /// Auto-refresh data from server
    pub async fn auto_refresh(&mut self) {
        // Handle pending config refresh request
        if self.needs_config_refresh && self.connected {
            self.send_message(ClientMessage::GetConfig).await;
            self.needs_config_refresh = false;
        }
        
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
                                self.send_message(ClientMessage::RemoveRoute(path.clone())).await;
                                self.send_message(ClientMessage::GetConfig).await;
                                self.add_message(&format!("Route '{}' removed", path), false);
                            }
                        }
                    }
                    Tab::Upstreams => {
                        if let Some(ref config) = self.config {
                            let names: Vec<_> = config.upstreams.keys().collect();
                            if let Some(name) = names.get(self.selected_upstream) {
                                let name = (*name).clone();
                                self.send_message(ClientMessage::RemoveUpstream(name.clone())).await;
                                self.send_message(ClientMessage::GetConfig).await;
                                self.add_message(&format!("Upstream '{}' removed", name), false);
                            }
                        }
                    }
                    Tab::Config => {
                        // Delete selected static directory
                        if let Some(ref config) = self.config {
                            if let Some(static_dir) = config.static_files.get(self.selected_static_dir) {
                                let path = static_dir.path.clone();
                                self.send_message(ClientMessage::RemoveStaticDir(path.clone())).await;
                                self.send_message(ClientMessage::GetConfig).await;
                                self.add_message(&format!("Static directory '{}' removed", path), false);
                            }
                        }
                    }
                    _ => {}
                }
            }
            
            // Add new item
            KeyCode::Char('a') => {
                if self.connected {
                    match self.current_tab {
                        Tab::Routes => {
                            // Check if there are any upstreams to route to
                            if let Some(ref config) = self.config {
                                if config.upstreams.is_empty() {
                                    self.add_message("Cannot add route: no upstreams configured", true);
                                } else {
                                    self.new_route = NewRoute::default();
                                    self.input_buffer.clear();
                                    self.edit_mode = EditMode::AddRoutePath;
                                    self.add_message("Enter route path (e.g., /api/* or /health)", false);
                                }
                            }
                        }
                        Tab::Config => {
                            // Add static directory
                            self.new_static_dir = NewStaticDir::default();
                            self.input_buffer.clear();
                            self.edit_mode = EditMode::AddStaticPath;
                            self.add_message("Enter URL path for static files (e.g., /static/)", false);
                        }
                        _ => {}
                    }
                }
            }
            
            // Edit port (on Config tab)
            KeyCode::Char('p') => {
                if self.current_tab == Tab::Config && self.connected {
                    if let Some(ref config) = self.config {
                        self.input_buffer = config.server.port.to_string();
                        self.edit_mode = EditMode::EditServerPort;
                        self.add_message(&format!("Enter new server port (current: {})", config.server.port), false);
                    }
                }
            }
            
            _ => {}
        }
    }

    /// Submit the current edit
    async fn submit_edit(&mut self) {
        match self.edit_mode {
            EditMode::AddRoutePath => {
                if self.input_buffer.is_empty() {
                    self.add_message("Path cannot be empty", true);
                    return;
                }
                self.new_route.path = self.input_buffer.clone();
                self.input_buffer.clear();
                self.edit_mode = EditMode::AddRouteUpstream;
                
                // Show available upstreams
                if let Some(ref config) = self.config {
                    let upstreams: Vec<_> = config.upstreams.keys().collect();
                    self.add_message(&format!("Enter upstream name. Available: {}", upstreams.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")), false);
                }
            }
            EditMode::AddRouteUpstream => {
                if self.input_buffer.is_empty() {
                    self.add_message("Upstream cannot be empty", true);
                    return;
                }
                // Validate upstream exists
                if let Some(ref config) = self.config {
                    if !config.upstreams.contains_key(&self.input_buffer) {
                        self.add_message(&format!("Upstream '{}' not found", self.input_buffer), true);
                        return;
                    }
                }
                self.new_route.upstream = self.input_buffer.clone();
                self.input_buffer = "30".to_string(); // Default timeout
                self.edit_mode = EditMode::AddRouteTimeout;
                self.add_message("Enter timeout in seconds (default: 30)", false);
            }
            EditMode::AddRouteTimeout => {
                let timeout: u64 = self.input_buffer.parse().unwrap_or(30);
                self.new_route.timeout = timeout.to_string();
                
                // Create and send the route
                let route = RouteConfig {
                    path: self.new_route.path.clone(),
                    methods: vec![], // All methods
                    upstream: self.new_route.upstream.clone(),
                    rewrite: None,
                    headers: HashMap::new(),
                    timeout,
                };
                
                self.send_message(ClientMessage::AddRoute(route)).await;
                self.send_message(ClientMessage::GetConfig).await;
                self.add_message(&format!("Route '{}' added successfully", self.new_route.path), false);
                
                // Reset state
                self.edit_mode = EditMode::None;
                self.input_buffer.clear();
                self.new_route = NewRoute::default();
            }
            EditMode::EditServerPort => {
                let port: u16 = match self.input_buffer.parse() {
                    Ok(p) if p > 0 => p,
                    _ => {
                        self.add_message("Invalid port number", true);
                        return;
                    }
                };
                
                self.send_message(ClientMessage::UpdateServerPort(port)).await;
                self.send_message(ClientMessage::GetConfig).await;
                
                // Reset state
                self.edit_mode = EditMode::None;
                self.input_buffer.clear();
            }
            EditMode::AddStaticPath => {
                if self.input_buffer.is_empty() {
                    self.add_message("Path cannot be empty", true);
                    return;
                }
                self.new_static_dir.path = self.input_buffer.clone();
                self.input_buffer.clear();
                self.edit_mode = EditMode::AddStaticRoot;
                self.add_message("Enter root directory path (e.g., /var/www/html)", false);
            }
            EditMode::AddStaticRoot => {
                if self.input_buffer.is_empty() {
                    self.add_message("Root directory cannot be empty", true);
                    return;
                }
                self.new_static_dir.root = self.input_buffer.clone();
                
                // Create and send the static config
                let static_config = StaticFileConfig {
                    path: self.new_static_dir.path.clone(),
                    root: self.new_static_dir.root.clone(),
                    index: "index.html".to_string(),
                    directory_listing: true,
                };
                
                self.send_message(ClientMessage::AddStaticDir(static_config)).await;
                self.send_message(ClientMessage::GetConfig).await;
                self.add_message(&format!("Static directory '{}' -> '{}' added", self.new_static_dir.path, self.new_static_dir.root), false);
                
                // Reset state
                self.edit_mode = EditMode::None;
                self.input_buffer.clear();
                self.new_static_dir = NewStaticDir::default();
            }
            EditMode::None => {}
        }
    }
    
    /// Get the current edit prompt
    pub fn get_edit_prompt(&self) -> &str {
        match self.edit_mode {
            EditMode::None => "",
            EditMode::AddRoutePath => "Route path: ",
            EditMode::AddRouteUpstream => "Upstream: ",
            EditMode::AddRouteTimeout => "Timeout (seconds): ",
            EditMode::EditServerPort => "Server port: ",
            EditMode::AddStaticPath => "URL path: ",
            EditMode::AddStaticRoot => "Root directory: ",
        }
    }
}
