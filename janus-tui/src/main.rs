//! Janus TUI - Terminal User Interface for managing Janus server

mod app;
mod client;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tracing::error;

/// Print help message and exit
fn print_help() {
    println!("janus-tui - Terminal User Interface for managing Janus server");
    println!();
    println!("USAGE:");
    println!("    janus-tui [OPTIONS] [SERVER_ADDR]");
    println!();
    println!("ARGS:");
    println!("    <SERVER_ADDR>    Server address to connect to [default: 127.0.0.1:9090]");
    println!();
    println!("OPTIONS:");
    println!("    -d, --debug      Enable debug logging to janus-tui.log");
    println!("    -h, --help       Print help information");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut debug_mode = false;
    let mut server_addr = "127.0.0.1:9090".to_string();

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-d" | "--debug" => {
                debug_mode = true;
            }
            _ if arg.starts_with('-') => {
                eprintln!("error: unknown option: {}", arg);
                eprintln!();
                print_help();
                std::process::exit(1);
            }
            _ => {
                // Assume non-flag arguments are the server address
                server_addr = arg.clone();
            }
        }
    }

    // Initialize logging - write to file when debug mode is enabled
    // We can't use stderr since the TUI uses the terminal in raw mode
    if debug_mode {
        let log_file = std::fs::File::create("janus-tui.log")?;
        tracing_subscriber::fmt()
            .with_writer(log_file)
            .with_env_filter("janus_tui=debug")
            .with_ansi(false)
            .init();
    } else {
        // When not in debug mode, discard logs by using a no-op subscriber
        tracing_subscriber::fmt()
            .with_writer(std::io::sink)
            .with_env_filter("janus_tui=warn")
            .init();
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new(server_addr);
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        error!("Application error: {}", e);
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    // Initial connection attempt
    app.connect().await;

    loop {
        // Draw UI
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for events with timeout
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Global quit handler
                if key.code == KeyCode::Char('q') && key.modifiers.is_empty() && !app.is_editing() {
                    return Ok(());
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }

                // Handle input
                app.handle_key(key).await;
            }
        }

        // Process any pending messages from server
        app.process_messages().await;

        // Auto-refresh status
        app.auto_refresh().await;
    }
}
