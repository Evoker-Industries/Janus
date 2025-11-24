# Janus

A lightweight, high-performance web server and reverse proxy written in Rust with live configuration reloading and a TUI management interface.

## Features

- **Web Server**: Serve static files with directory listing support
- **Reverse Proxy**: Proxy requests to upstream servers with load balancing
- **Live Reloading**: Configuration changes are automatically detected and applied without restart
- **TUI Management**: Interactive terminal interface for real-time server management
- **Easy Configuration**: Simple TOML-based configuration

## Components

### janus (Server)

The main web server and reverse proxy daemon.

```bash
# Run with default config (./janus.toml)
cargo run --bin janus

# Run with custom config
cargo run --bin janus -- /path/to/config.toml
```

### janus-tui (Management Interface)

Terminal UI for managing the running Janus server.

```bash
# Connect to default address (127.0.0.1:9090)
cargo run --bin janus-tui

# Connect to custom address
cargo run --bin janus-tui -- 192.168.1.100:9090
```

## Configuration

Janus uses TOML for configuration. Here's a complete example:

```toml
[server]
bind_address = "0.0.0.0"
port = 8080
workers = 0  # 0 = auto-detect
access_log = true

[management]
enabled = true
address = "127.0.0.1"
port = 9090

# Define upstream servers for reverse proxy
[upstreams.backend]
servers = [
    { address = "localhost:3001", weight = 1 },
    { address = "localhost:3002", weight = 2 }
]
load_balancing = "round_robin"  # round_robin, least_connections, random, ip_hash

[upstreams.backend.health_check]
interval = 30
timeout = 5
path = "/health"

# Define routes
[[routes]]
path = "/api/*"
methods = ["GET", "POST", "PUT", "DELETE"]
upstream = "backend"
rewrite = "/v1"
timeout = 30

[routes.headers]
X-Forwarded-For = "$remote_addr"

# Static file serving
[[static_files]]
path = "/"
root = "/var/www/html"
index = "index.html"
directory_listing = false
```

## Live Reloading

Janus supports two methods of live configuration reloading:

### 1. Automatic File Watching

The server automatically watches the configuration file for changes. Simply edit and save the file, and changes will be applied within 500ms.

### 2. Via Management API

Use the TUI or send commands directly to trigger a configuration reload:

- **TUI**: Press `R` to reload configuration
- **WebSocket**: Send `{"type": "ReloadConfig"}` message

## TUI Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch between tabs |
| `1-6` | Jump to specific tab |
| `j/k` or `↑/↓` | Navigate lists |
| `r` | Refresh data from server |
| `R` | Reload server configuration |
| `c` | Reconnect to server |
| `d` / `Delete` | Delete selected item |
| `q` | Quit |

## Architecture

```
┌─────────────────┐     ┌─────────────────┐
│   janus-tui     │────▶│  Management API │
│  (TUI Client)   │◀────│  (WebSocket)    │
└─────────────────┘     └────────┬────────┘
                                 │
                        ┌────────▼────────┐
                        │  janus-server   │
                        │                 │
                        │  ┌───────────┐  │
┌──────────────┐        │  │  Config   │  │
│ Config File  │───────▶│  │  Manager  │  │
│ (janus.toml) │ watch  │  └───────────┘  │
└──────────────┘        │                 │
                        │  ┌───────────┐  │
                        │  │   HTTP    │  │
    HTTP Requests ─────▶│  │  Server   │  │
                        │  └─────┬─────┘  │
                        │        │        │
                        │  ┌─────▼─────┐  │
                        │  │   Proxy   │  │──────▶ Upstream Servers
                        │  │  Handler  │  │
                        │  └───────────┘  │
                        │                 │
                        │  ┌───────────┐  │
                        │  │  Static   │  │
                        │  │   Files   │  │
                        │  └───────────┘  │
                        └─────────────────┘
```

## Building

```bash
# Build all components
cargo build --release

# The binaries will be at:
# - target/release/janus
# - target/release/janus-tui
```

## License

MIT License