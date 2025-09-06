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
        
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Use a timeout so we can update UI while discovery runs
        if let Ok(true) = event::poll(Duration::from_millis(100)) {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('?') => app.toggle_help(),
                    KeyCode::Char(',') => app.toggle_settings(),
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
                                        // Show confirmation by temporarily updating last_error
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
                    _ => {
                        // Handle settings input if settings dialog is open
                        if app.show_settings {
                            if app.settings_editing {
                                // Handle text input with ratatui_input
                                app.handle_settings_input(&key);
                                
                                // Handle special keys
                                match key.code {
                                    KeyCode::Enter => {
                                        if let Err(e) = app.save_settings() {
                                            app.last_error = Some(format!("Failed to save settings: {}", e));
                                        }
                                    }
                                    KeyCode::Esc => {
                                        app.cancel_editing_settings();
                                    }
                                    _ => {}
                                }
                            } else {
                                // Navigation mode
                                match key.code {
                                    KeyCode::Char('e') => {
                                        app.start_editing_settings();
                                    }
                                    KeyCode::Tab => {
                                        app.next_settings_field();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
