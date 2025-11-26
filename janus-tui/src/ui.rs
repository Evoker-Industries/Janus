//! TUI rendering

use crate::app::{App, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};

/// Main draw function
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header/tabs
            Constraint::Min(10),   // Main content
            Constraint::Length(5), // Status messages
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    draw_tabs(f, app, chunks[0]);
    draw_main_content(f, app, chunks[1]);
    draw_messages(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);
}

/// Draw tab bar
fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::all()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Line::from(vec![
                Span::styled(format!("{}:", i + 1), Style::default().fg(Color::Yellow)),
                Span::raw(t.name()),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Janus TUI"))
        .select(
            Tab::all()
                .iter()
                .position(|&t| t == app.current_tab)
                .unwrap_or(0),
        )
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

/// Draw main content based on current tab
fn draw_main_content(f: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Status => draw_status(f, app, area),
        Tab::Routes => draw_routes(f, app, area),
        Tab::Upstreams => draw_upstreams(f, app, area),
        Tab::Config => draw_config(f, app, area),
        Tab::Stats => draw_stats(f, app, area),
        Tab::Help => draw_help(f, area),
    }
}

/// Draw status tab
fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let connection_status = if app.connected {
        Span::styled("● Connected", Style::default().fg(Color::Green))
    } else {
        Span::styled("● Disconnected", Style::default().fg(Color::Red))
    };

    let mut lines = vec![
        Line::from(vec![Span::raw("Connection: "), connection_status]),
        Line::from(vec![
            Span::raw("Server: "),
            Span::styled(&app.server_addr, Style::default().fg(Color::Cyan)),
        ]),
        Line::raw(""),
    ];

    if let Some(ref status) = app.status {
        lines.extend(vec![
            Line::from(vec![
                Span::raw("Version: "),
                Span::styled(&status.version, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("Listen Address: "),
                Span::styled(&status.listen_address, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("Uptime: "),
                Span::styled(
                    format_duration(status.uptime_secs),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Active Connections: "),
                Span::styled(
                    status.active_connections.to_string(),
                    Style::default().fg(Color::Magenta),
                ),
            ]),
            Line::from(vec![
                Span::raw("Routes: "),
                Span::styled(
                    status.route_count.to_string(),
                    Style::default().fg(Color::Blue),
                ),
            ]),
            Line::from(vec![
                Span::raw("Upstreams: "),
                Span::styled(
                    status.upstream_count.to_string(),
                    Style::default().fg(Color::Blue),
                ),
            ]),
        ]);
    } else {
        lines.push(Line::styled(
            "No status data available",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Server Status"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draw routes tab
fn draw_routes(f: &mut Frame, app: &App, area: Rect) {
    let header_cells = ["Path", "Methods", "Upstream", "Timeout"].iter().map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = if let Some(ref config) = app.config {
        config
            .routes
            .iter()
            .enumerate()
            .map(|(i, route)| {
                let methods = if route.methods.is_empty() {
                    "ALL".to_string()
                } else {
                    route.methods.join(", ")
                };

                let style = if i == app.selected_route {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(route.path.clone()),
                    Cell::from(methods),
                    Cell::from(route.upstream.clone()),
                    Cell::from(format!("{}s", route.timeout)),
                ])
                .style(style)
            })
            .collect()
    } else {
        vec![]
    };

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Routes (a: add, d: delete, j/k: navigate)"),
    );

    f.render_widget(table, area);
}

/// Draw upstreams tab
fn draw_upstreams(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if let Some(ref config) = app.config {
        config
            .upstreams
            .iter()
            .enumerate()
            .map(|(i, (name, upstream))| {
                let servers = upstream
                    .servers
                    .iter()
                    .map(|s| format!("{} (weight: {})", s.address, s.weight))
                    .collect::<Vec<_>>()
                    .join(", ");

                let style = if i == app.selected_upstream {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                ListItem::new(vec![
                    Line::from(vec![Span::styled(
                        name,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![
                        Span::raw("  Servers: "),
                        Span::styled(servers, Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::raw("  Load Balancing: "),
                        Span::styled(
                            format!("{:?}", upstream.load_balancing),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]),
                ])
                .style(style)
            })
            .collect()
    } else {
        vec![]
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Upstreams (a: add, d: delete, j/k: navigate)"),
    );

    f.render_widget(list, area);
}

/// Draw config tab
fn draw_config(f: &mut Frame, app: &App, area: Rect) {
    // Split into two sections: server settings and static directories
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Server settings
            Constraint::Min(5),    // Static directories
        ])
        .split(area);

    // Server settings
    let mut server_lines = vec![
        Line::styled(
            "Server Settings",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        ),
        Line::raw(""),
    ];

    if let Some(ref config) = app.config {
        server_lines.extend(vec![
            Line::from(vec![
                Span::raw("  Bind Address: "),
                Span::styled(
                    &config.server.bind_address,
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Port: "),
                Span::styled(
                    config.server.port.to_string(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    " (press 'p' to change)",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Access Log: "),
                Span::styled(
                    if config.server.access_log {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    Style::default().fg(Color::Green),
                ),
            ]),
        ]);
    } else {
        server_lines.push(Line::styled(
            "No configuration loaded",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let server_para = Paragraph::new(server_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Server (p: edit port, R: reload config)"),
    );
    f.render_widget(server_para, chunks[0]);

    // Static directories
    let header_cells = ["URL Path", "Root Directory", "Index", "Dir Listing"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = if let Some(ref config) = app.config {
        config
            .static_files
            .iter()
            .enumerate()
            .map(|(i, sf)| {
                let style = if i == app.selected_static_dir {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(sf.path.clone()),
                    Cell::from(sf.root.clone()),
                    Cell::from(sf.index.clone()),
                    Cell::from(if sf.directory_listing { "Yes" } else { "No" }),
                ])
                .style(style)
            })
            .collect()
    } else {
        vec![]
    };

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Static Directories (a: add, d: delete, j/k: navigate)"),
    );

    f.render_widget(table, chunks[1]);
}

/// Draw stats tab
fn draw_stats(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![];

    if let Some(ref stats) = app.stats {
        lines.extend(vec![
            Line::from(vec![
                Span::raw("Total Requests: "),
                Span::styled(
                    stats.total_requests.to_string(),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Requests/sec: "),
                Span::styled(
                    format!("{:.2}", stats.requests_per_second),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("Bytes Received: "),
                Span::styled(
                    format_bytes(stats.bytes_received),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::raw("Bytes Sent: "),
                Span::styled(
                    format_bytes(stats.bytes_sent),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::raw(""),
            Line::styled(
                "Status Codes:",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::from(vec![
                Span::raw("  2xx (Success): "),
                Span::styled(
                    stats.status_codes.success.to_string(),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("  3xx (Redirect): "),
                Span::styled(
                    stats.status_codes.redirect.to_string(),
                    Style::default().fg(Color::Blue),
                ),
            ]),
            Line::from(vec![
                Span::raw("  4xx (Client Error): "),
                Span::styled(
                    stats.status_codes.client_error.to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("  5xx (Server Error): "),
                Span::styled(
                    stats.status_codes.server_error.to_string(),
                    Style::default().fg(Color::Red),
                ),
            ]),
        ]);
    } else {
        lines.push(Line::styled(
            "No statistics available",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Statistics"))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draw help tab
fn draw_help(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::styled("Navigation", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  Tab / Shift+Tab - Switch between tabs"),
        Line::raw("  1-6            - Jump to specific tab"),
        Line::raw("  j/k or ↑/↓     - Navigate lists"),
        Line::raw(""),
        Line::styled(
            "Global Actions",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::raw("  r              - Refresh data from server"),
        Line::raw("  R              - Reload server configuration from file"),
        Line::raw("  c              - Reconnect to server"),
        Line::raw("  q              - Quit"),
        Line::raw(""),
        Line::styled("Routes Tab", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  a              - Add new route"),
        Line::raw("  d / Delete     - Delete selected route"),
        Line::raw(""),
        Line::styled("Upstreams Tab", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  a              - Add new upstream"),
        Line::raw("  d / Delete     - Delete selected upstream"),
        Line::raw(""),
        Line::styled("Config Tab", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  p              - Edit server port"),
        Line::raw("  a              - Add static directory"),
        Line::raw("  d / Delete     - Delete selected static directory"),
        Line::raw(""),
        Line::styled("Editing", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  Enter          - Confirm input"),
        Line::raw("  Esc            - Cancel editing"),
        Line::raw(""),
        Line::styled("Notes", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("  Port changes require server restart to take effect."),
        Line::raw("  Route, upstream, and static directory changes apply immediately."),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draw status messages
fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .rev()
        .take(3)
        .map(|msg| {
            let style = if msg.is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            ListItem::new(Line::styled(&msg.text, style))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Messages"));

    f.render_widget(list, area);
}

/// Draw footer
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let footer = if app.is_editing() {
        if let Some((options, selected)) = app.get_dropdown_options() {
            // Render dropdown menu
            let options_display: Vec<Span> = options
                .iter()
                .enumerate()
                .flat_map(|(i, opt)| {
                    let style = if i == selected {
                        Style::default().fg(Color::Black).bg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    vec![
                        Span::styled(format!(" {} ", opt), style),
                        Span::raw(" "),
                    ]
                })
                .collect();
            
            Paragraph::new(Line::from(
                std::iter::once(Span::styled(app.get_edit_prompt(), Style::default().fg(Color::Yellow)))
                    .chain(options_display)
                    .collect::<Vec<_>>()
            ))
        } else {
            Paragraph::new(format!("{}{}_", app.get_edit_prompt(), app.input_buffer))
                .style(Style::default().fg(Color::Yellow))
        }
    } else {
        // Context-sensitive footer message
        let context_hint = match app.current_tab {
            Tab::Routes => "'a' add route",
            Tab::Upstreams => "'a' add upstream",
            Tab::Config => "'a' add static dir",
            _ => "'a' add item",
        };
        Paragraph::new(format!(
            "Press 'q' to quit | Tab to switch views | 'r' to refresh | {}",
            context_hint
        ))
        .style(Style::default().fg(Color::DarkGray))
    };

    f.render_widget(footer, area);
}

/// Format duration in human-readable form
fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format bytes in human-readable form
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
