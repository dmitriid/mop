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
mod ui;
mod upnp;
mod upnp_ssdp;
mod macos_permissions;
mod network_interfaces;
mod discovery_manager;
mod debug_ssdp;

use app::App;

fn main() -> Result<(), Box<dyn Error>> {
    // Check for debug mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "debug" {
        debug_ssdp::debug_ssdp_discovery();
        debug_ssdp::test_multicast_methods();
        return Ok(());
    }
    
    // Check and handle macOS permissions before starting TUI
    #[cfg(target_os = "macos")]
    {
        use macos_permissions::{check_local_network_permission, request_permission_interactive, PermissionState};
        
        let permission_state = check_local_network_permission();
        match permission_state {
            PermissionState::Denied => {
                println!("⚠️  Local network permission is required for UPnP discovery.");
                match request_permission_interactive() {
                    Ok(PermissionState::Granted) => {
                        println!("✅ Permission granted! Starting application...\n");
                    }
                    Ok(_) | Err(_) => {
                        println!("⚠️  Continuing without permission. UPnP discovery may not work.");
                        println!("💡 You can grant permission later in System Preferences.\n");
                        println!("💡 Run 'cargo run debug' to test SSDP discovery in detail.\n");
                    }
                }
            }
            PermissionState::Unknown => {
                println!("🔍 Checking network permissions...");
            }
            PermissionState::Granted => {
                // All good, proceed normally
            }
            PermissionState::NeedsRequest => {
                // Will be handled during discovery
            }
        }
    }
    
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
                    _ => {}
                }
            }
        }
    }
}
