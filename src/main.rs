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

use ollama_tui::app::{Action, App};
use ollama_tui::config::Config;
use ollama_tui::ui::ui;
use std::fs::OpenOptions; // For debug logging
use std::io::Write; // For debug logging & flush

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Debug Log Setup
    let _ = std::fs::write("debug_log.txt", "Starting app...\n");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    // Load configuration
    let config = Config::load().unwrap_or_else(|e| {
        eprintln!(
            "Warning: Failed to load config, using defaults. Error: {}",
            e
        );
        Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are a helpful AI assistant with access to local system tools. You can read/write files and run commands. Use these tools whenever real-world interaction is needed.".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        }
    });

    let mut app = App::new(action_tx.clone(), config, true, None);

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
    let _ = std::fs::OpenOptions::new().append(true).open("debug_log.txt").map(|mut f| writeln!(f, "Restoring terminal..."));
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = std::fs::OpenOptions::new().append(true).open("debug_log.txt").map(|mut f| writeln!(f, "Aborting input task..."));
    // Explicitly abort the input task to ensure the process exits
    input_handle.abort();

    let _ = std::fs::OpenOptions::new().append(true).open("debug_log.txt").map(|mut f| writeln!(f, "Killing child processes..."));
    // Kill any lingering child processes spawned by tools
    app.process_tracker.kill_all();

    let _ = std::fs::OpenOptions::new().append(true).open("debug_log.txt").map(|mut f| writeln!(f, "Flushing stdout..."));
    let _ = std::io::stdout().flush();

    let _ = std::fs::OpenOptions::new().append(true).open("debug_log.txt").map(|mut f| writeln!(f, "Exiting..."));

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
