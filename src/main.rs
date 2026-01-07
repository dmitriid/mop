use std::error::Error;
use std::io;
use std::time::Duration;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    Terminal,
};

mod app;
mod config;
mod logger;
mod ui;
mod upnp;

use app::App;

fn main() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new();
    app.start_discovery();
    let res = run_app(&mut terminal, app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}


fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> io::Result<()> {
    loop {
        // Check for discovery updates
        app.check_discovery_updates();
        
        // Check if we should quit (for auto-close)
        if app.should_quit {
            return Ok(());
        }
        
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Use a timeout so we can update UI while discovery runs
        if let Ok(true) = event::poll(Duration::from_millis(100)) {
            if let Event::Key(key) = event::read()? {

                
                // Handle config modal first
                if app.show_config {
                    match key.code {
                        KeyCode::Esc => app.cancel_config_edit(),
                        KeyCode::Enter => {
                            if let Err(e) = app.save_config() {
                                app.last_error = Some(e);
                            }
                        }
                        KeyCode::Tab => app.config_editor.next_field(),
                        KeyCode::BackTab => app.config_editor.previous_field(),
                        KeyCode::Char(' ') => app.config_editor.toggle_auto_close(),
                        _ => {
                            app.config_editor.handle_key(key);
                        }
                    }
                    continue;
                }

                // Handle help modal next
                if app.show_help {
                    match key.code {
                        KeyCode::Char('?') | KeyCode::Esc => {
                            app.toggle_help();
                            continue;
                        }
                        _ => continue, // Block other keys while help is shown
                    }
                }
                
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('?') => app.toggle_help(),
                    KeyCode::Char('c') => app.open_config_editor(),
                            KeyCode::Char('e') => {
                                // Copy errors to system clipboard
                                if !app.discovery_errors.is_empty() {
                                    let errors_text = app.discovery_errors.iter()
                                        .enumerate()
                                        .map(|(i, error)| format!("{}. {}", i + 1, error))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    
                                    match arboard::Clipboard::new() {
                                        Ok(mut clipboard) => {
                                            if clipboard.set_text(&errors_text).is_ok() {
                                                app.last_error = Some("Errors copied to clipboard".to_string());
                                            } else {
                                                app.last_error = Some("Failed to copy to clipboard".to_string());
                                            }
                                        }
                                        Err(_) => {
                                            app.last_error = Some("Clipboard not available".to_string());
                                        }
                                    }
                                }
                            }
                    KeyCode::Up => app.previous(),
                    KeyCode::Down => app.next(),
                    KeyCode::Enter => app.select(),
                    KeyCode::Backspace => app.go_back(),
                    _ => {}
                }
            }
        }
    }
}
