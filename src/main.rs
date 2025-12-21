//! # Ollama TUI Application Entry Point
//!
//! This file contains the `main` function which sets up the application environment,
//! initializes the TUI backend, loads configuration, and starts the main event loop.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::io;
use tokio::sync::mpsc;

use intus::app::{Action, App};
use intus::config::Config;
use intus::ui::ui;
use intus::logging;
use tracing::{info, warn};
use std::io::Write;

use clap::Parser;

/// A robust, privacy-first local AI assistant and system sidecar.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {}

/// The main entry point for the Ollama TUI application.
///
/// This function:
/// 1. Initializes logging.
/// 2. Sets up the terminal in raw mode with mouse support.
/// 3. Loads the application configuration.
/// 4. Spawns a background task for handling input events.
/// 5. Runs the main application loop.
/// 6. Cleans up the terminal state upon exit.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _cli = Cli::parse();

    // Initialize logging
    let _ = logging::init_logging();
    info!("Starting Intus");

    // Load config
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    // Load configuration
    let config = Config::load().unwrap_or_else(|e| {
        warn!("Failed to load config, using defaults. Error: {}", e);
        Config::new_test_config()
    });

    let mut app = App::init(action_tx.clone(), config, true, None).await;

    // Input handling task
    let input_handle = {
        let tx = action_tx.clone();
        tokio::spawn(async move {
            loop {
                if event::poll(std::time::Duration::from_millis(100)).unwrap() {
                    match event::read().unwrap() {
                        Event::Key(key) if key.kind == KeyEventKind::Press => {
                            let _ = tx.send(Action::UserInput(key));
                        }
                        Event::Mouse(mouse) => match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                let _ = tx.send(Action::Scroll(-3));
                            }
                            MouseEventKind::ScrollDown => {
                                let _ = tx.send(Action::Scroll(3));
                            }
                            _ => {}
                        },
                        Event::Resize(w, h) => {
                            let _ = tx.send(Action::Resize(w, h));
                        }
                        _ => {}
                    }
                } else {
                    // Tick for spinner
                    let _ = tx.send(Action::Render);
                }
            }
        })
    };

    // Initial load
    let _ = action_tx.send(Action::LoadModels);

    let res = run_app(&mut terminal, &mut app, &mut action_rx, action_tx.clone()).await;

    // Restore
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Explicitly abort the input task to ensure the process exits
    input_handle.abort();

    // Kill any lingering child processes spawned by tools
    app.process_tracker.kill_all();

    // Ensure session is saved
    app.wait_for_save().await;

    // Ensure terminal buffer is flushed before exit
    let _ = std::io::stdout().flush();

    if let Err(err) = res {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    std::process::exit(0);
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App<'_>,
    action_rx: &mut mpsc::UnboundedReceiver<Action>,
    action_tx: mpsc::UnboundedSender<Action>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_millis(100);

    loop {
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));

        tokio::select! {
            Some(action) = action_rx.recv() => {
                 match action {
                    Action::Render => {
                         // Force render
                         terminal.draw(|f| ui(f, app))?;
                    }
                    Action::Resize(_, _) => terminal.autoresize()?,
                    Action::Quit => return Ok(()),
                    _ => {
                        if app.update(action).await {
                             terminal.draw(|f| ui(f, app))?;
                        }
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                 if app.loading {
                     app.spinner_state.calc_next();
                     terminal.draw(|f| ui(f, app))?;
                 }
                 last_tick = std::time::Instant::now();
            }
            // Handle Ctrl+C gracefully - save session before exiting
            _ = tokio::signal::ctrl_c() => {
                let _ = action_tx.send(Action::PrepareQuit);
            }
        }
    }
}
