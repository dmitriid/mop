# Log Pane Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a togglable debug log pane with verbose logging to diagnose UPnP discovery issues across Linux distributions.

**Architecture:** Custom `log` crate backend captures all log messages into a thread-safe ring buffer. The UI reads from this buffer to render a scrollable, filterable log pane with three toggle states (off/bottom/fullscreen).

**Tech Stack:** `log` crate for logging facade, `chrono` for timestamps, existing `ratatui` for UI, `Arc<Mutex<VecDeque>>` for thread-safe buffer.

---

## Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add log and chrono crates**

Run:
```bash
cd /home/dmitriid/Projects/mop/.worktrees/log-pane && cargo add log chrono
```

**Step 2: Verify build**

Run: `cargo build 2>&1 | grep -E "error|warning: unused"`
Expected: No new errors (existing warnings OK)

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add log and chrono dependencies for log pane"
```

---

## Task 2: Create Logger Module - Data Types

**Files:**
- Create: `src/logger.rs`
- Modify: `src/main.rs` (add mod declaration)

**Step 1: Create logger.rs with data types**

Create `src/logger.rs`:
```rust
use chrono::{DateTime, Local};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogCategory {
    Net,
    Disc,
    Soap,
    Http,
    Xml,
    App,
}

impl LogCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogCategory::Net => "NET",
            LogCategory::Disc => "DISC",
            LogCategory::Soap => "SOAP",
            LogCategory::Http => "HTTP",
            LogCategory::Xml => "XML",
            LogCategory::App => "APP",
        }
    }

    fn from_target(target: &str) -> Self {
        let target_lower = target.to_lowercase();
        if target_lower.contains("net") || target_lower.contains("socket") || target_lower.contains("multicast") {
            LogCategory::Net
        } else if target_lower.contains("upnp") || target_lower.contains("disc") || target_lower.contains("rupnp") || target_lower.contains("ssdp") {
            LogCategory::Disc
        } else if target_lower.contains("soap") {
            LogCategory::Soap
        } else if target_lower.contains("http") || target_lower.contains("reqwest") {
            LogCategory::Http
        } else if target_lower.contains("xml") || target_lower.contains("quick_xml") {
            LogCategory::Xml
        } else {
            LogCategory::App
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogSeverity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogSeverity::Error => "ERROR",
            LogSeverity::Warn => "WARN",
            LogSeverity::Info => "INFO",
            LogSeverity::Debug => "DEBUG",
            LogSeverity::Trace => "TRACE",
        }
    }
}

impl From<log::Level> for LogSeverity {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogSeverity::Error,
            log::Level::Warn => LogSeverity::Warn,
            log::Level::Info => LogSeverity::Info,
            log::Level::Debug => LogSeverity::Debug,
            log::Level::Trace => LogSeverity::Trace,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub category: LogCategory,
    pub severity: LogSeverity,
    pub message: String,
}

impl LogEntry {
    pub fn format_line(&self) -> String {
        format!(
            "{} [{}] {}",
            self.timestamp.format("%H:%M:%S"),
            self.category.as_str(),
            self.message
        )
    }

    pub fn format_export_line(&self) -> String {
        format!(
            "{} [{}] {:5} {}",
            self.timestamp.format("%H:%M:%S"),
            self.category.as_str(),
            self.severity.as_str(),
            self.message
        )
    }
}

pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

pub const LOG_BUFFER_CAPACITY: usize = 2000;
```

**Step 2: Add mod declaration to main.rs**

In `src/main.rs`, after line 17 (`mod upnp;`), add:
```rust
mod logger;
```

**Step 3: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 4: Commit**

```bash
git add src/logger.rs src/main.rs
git commit -m "feat: add logger module with data types"
```

---

## Task 3: Create Logger Module - Ring Buffer Logger

**Files:**
- Modify: `src/logger.rs`

**Step 1: Add RingBufferLogger implementation**

Append to `src/logger.rs`:
```rust

pub struct RingBufferLogger {
    buffer: LogBuffer,
}

impl RingBufferLogger {
    pub fn new() -> (Self, LogBuffer) {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_BUFFER_CAPACITY)));
        let buffer_handle = Arc::clone(&buffer);
        (Self { buffer }, buffer_handle)
    }
}

impl log::Log for RingBufferLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Trace
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let entry = LogEntry {
            timestamp: Local::now(),
            category: LogCategory::from_target(record.target()),
            severity: LogSeverity::from(record.level()),
            message: record.args().to_string(),
        };

        if let Ok(mut buffer) = self.buffer.lock() {
            if buffer.len() >= LOG_BUFFER_CAPACITY {
                buffer.pop_front();
            }
            buffer.push_back(entry);
        }
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<RingBufferLogger> = OnceLock::new();

pub fn init_logger() -> LogBuffer {
    let (logger, buffer) = RingBufferLogger::new();

    if LOGGER.set(logger).is_ok() {
        if let Some(logger) = LOGGER.get() {
            log::set_logger(logger).expect("Failed to set logger");
            log::set_max_level(log::LevelFilter::Trace);
        }
    }

    buffer
}
```

**Step 2: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 3: Commit**

```bash
git add src/logger.rs
git commit -m "feat: implement RingBufferLogger with log crate integration"
```

---

## Task 4: Initialize Logger in Main

**Files:**
- Modify: `src/main.rs`
- Modify: `src/app.rs`

**Step 1: Update main() to initialize logger**

In `src/main.rs`, replace the `main()` function (lines 21-48) with:
```rust
fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logger first
    let log_buffer = logger::init_logger();

    log::info!(target: "mop::app", "MOP starting up");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new(log_buffer);
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
```

**Step 2: Update App::new() to accept log buffer**

In `src/app.rs`, add import at the top (after line 5):
```rust
use crate::logger::LogBuffer;
```

Then update the `App` struct (after line 32, before closing brace) to add:
```rust
    pub log_buffer: LogBuffer,
```

Then update `App::new()` signature and body. Replace `pub fn new() -> Self {` with:
```rust
    pub fn new(log_buffer: LogBuffer) -> Self {
```

And in the `Self { ... }` block, add after `config_editor,`:
```rust
            log_buffer,
```

**Step 3: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 4: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: initialize logger and pass buffer to App"
```

---

## Task 5: Add Log Pane State to App

**Files:**
- Modify: `src/app.rs`

**Step 1: Add LogPaneState enum and fields**

In `src/app.rs`, after the `ConfigField` enum (after line 45), add:
```rust

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogPaneState {
    Hidden,
    Bottom,
    Fullscreen,
}

impl LogPaneState {
    pub fn next(self) -> Self {
        match self {
            LogPaneState::Hidden => LogPaneState::Bottom,
            LogPaneState::Bottom => LogPaneState::Fullscreen,
            LogPaneState::Fullscreen => LogPaneState::Hidden,
        }
    }
}
```

**Step 2: Add log pane fields to App struct**

In the `App` struct, after `pub log_buffer: LogBuffer,` add:
```rust
    pub log_pane_state: LogPaneState,
    pub log_scroll_offset: usize,
    pub log_filter: String,
    pub log_filter_input: String,
    pub log_filter_active: bool,
    pub log_auto_scroll: bool,
```

**Step 3: Initialize log pane fields in App::new()**

In `App::new()`, in the `Self { ... }` block, after `log_buffer,` add:
```rust
            log_pane_state: LogPaneState::Hidden,
            log_scroll_offset: 0,
            log_filter: String::new(),
            log_filter_input: String::new(),
            log_filter_active: false,
            log_auto_scroll: true,
```

**Step 4: Add log pane methods to App impl**

At the end of `impl App` (before the closing `}`), add:
```rust

    pub fn toggle_log_pane(&mut self) {
        self.log_pane_state = self.log_pane_state.next();
        if self.log_pane_state == LogPaneState::Hidden {
            self.log_filter_active = false;
        }
    }

    pub fn close_log_pane(&mut self) {
        self.log_pane_state = LogPaneState::Hidden;
        self.log_filter_active = false;
    }

    pub fn log_scroll_up(&mut self) {
        if self.log_scroll_offset > 0 {
            self.log_scroll_offset -= 1;
            self.log_auto_scroll = false;
        }
    }

    pub fn log_scroll_down(&mut self) {
        self.log_scroll_offset += 1;
        // Auto-scroll re-enabled by jump_to_bottom
    }

    pub fn log_jump_to_top(&mut self) {
        self.log_scroll_offset = 0;
        self.log_auto_scroll = false;
    }

    pub fn log_jump_to_bottom(&mut self) {
        self.log_scroll_offset = usize::MAX; // Will be clamped in UI
        self.log_auto_scroll = true;
    }

    pub fn start_log_filter(&mut self) {
        self.log_filter_active = true;
        self.log_filter_input = self.log_filter.clone();
    }

    pub fn confirm_log_filter(&mut self) {
        self.log_filter = self.log_filter_input.clone();
        self.log_filter_active = false;
        self.log_scroll_offset = 0;
    }

    pub fn cancel_log_filter(&mut self) {
        self.log_filter_input = self.log_filter.clone();
        self.log_filter_active = false;
    }

    pub fn get_filtered_logs(&self) -> Vec<crate::logger::LogEntry> {
        if let Ok(buffer) = self.log_buffer.lock() {
            if self.log_filter.is_empty() {
                buffer.iter().cloned().collect()
            } else {
                let filter_lower = self.log_filter.to_lowercase();
                buffer
                    .iter()
                    .filter(|entry| {
                        entry.format_line().to_lowercase().contains(&filter_lower)
                    })
                    .cloned()
                    .collect()
            }
        } else {
            Vec::new()
        }
    }

    pub fn export_logs(&self) -> Result<String, String> {
        use std::io::Write;

        let logs = if let Ok(buffer) = self.log_buffer.lock() {
            buffer.iter().cloned().collect::<Vec<_>>()
        } else {
            return Err("Failed to access log buffer".to_string());
        };

        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| "Could not find cache directory".to_string())?
            .join("mop");

        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        let filename = format!(
            "debug-{}.log",
            chrono::Local::now().format("%Y-%m-%d-%H%M%S")
        );
        let filepath = cache_dir.join(&filename);

        let mut file = std::fs::File::create(&filepath)
            .map_err(|e| format!("Failed to create log file: {}", e))?;

        writeln!(
            file,
            "MOP Debug Log - Exported {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
        .map_err(|e| format!("Write error: {}", e))?;

        writeln!(file, "Filter: {}", if self.log_filter.is_empty() { "(none)" } else { &self.log_filter })
            .map_err(|e| format!("Write error: {}", e))?;

        writeln!(file, "Entries: {}", logs.len())
            .map_err(|e| format!("Write error: {}", e))?;

        writeln!(file, "\n---")
            .map_err(|e| format!("Write error: {}", e))?;

        for entry in &logs {
            writeln!(file, "{}", entry.format_export_line())
                .map_err(|e| format!("Write error: {}", e))?;
        }

        Ok(filepath.to_string_lossy().to_string())
    }
```

**Step 5: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: add log pane state and methods to App"
```

---

## Task 6: Add Log Pane Key Bindings

**Files:**
- Modify: `src/main.rs`

**Step 1: Add log pane key handling**

In `src/main.rs`, in `run_app()`, find the section after help modal handling (after line 99 `_ => continue,`).

Before the main `match key.code {` block (around line 101), add log pane handling:
```rust
                // Handle log pane keys when visible
                if app.log_pane_state != crate::app::LogPaneState::Hidden {
                    // Filter input mode
                    if app.log_filter_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.cancel_log_filter();
                                continue;
                            }
                            KeyCode::Enter => {
                                app.confirm_log_filter();
                                continue;
                            }
                            KeyCode::Backspace => {
                                app.log_filter_input.pop();
                                continue;
                            }
                            KeyCode::Char(c) => {
                                app.log_filter_input.push(c);
                                continue;
                            }
                            _ => continue,
                        }
                    }

                    // Normal log pane keys
                    match key.code {
                        KeyCode::Char('l') => {
                            app.toggle_log_pane();
                            continue;
                        }
                        KeyCode::Esc => {
                            app.close_log_pane();
                            continue;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.log_scroll_up();
                            continue;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.log_scroll_down();
                            continue;
                        }
                        KeyCode::Char('t') => {
                            app.log_jump_to_top();
                            continue;
                        }
                        KeyCode::Char('b') => {
                            app.log_jump_to_bottom();
                            continue;
                        }
                        KeyCode::Char('/') => {
                            app.start_log_filter();
                            continue;
                        }
                        KeyCode::Char('s') => {
                            match app.export_logs() {
                                Ok(path) => {
                                    log::info!(target: "mop::app", "Exported logs to {}", path);
                                }
                                Err(e) => {
                                    log::error!(target: "mop::app", "Failed to export logs: {}", e);
                                }
                            }
                            continue;
                        }
                        KeyCode::PageUp => {
                            for _ in 0..10 {
                                app.log_scroll_up();
                            }
                            continue;
                        }
                        KeyCode::PageDown => {
                            for _ in 0..10 {
                                app.log_scroll_down();
                            }
                            continue;
                        }
                        _ => {} // Fall through to main key handling
                    }
                }

```

**Step 2: Add 'l' key to main key handler**

In the main `match key.code {` block, after `KeyCode::Char('c') => app.open_config_editor(),` add:
```rust
                    KeyCode::Char('l') => app.toggle_log_pane(),
```

**Step 3: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add log pane key bindings"
```

---

## Task 7: Add Log Pane UI Rendering

**Files:**
- Modify: `src/ui.rs`

**Step 1: Add log pane imports and constants**

At the top of `src/ui.rs`, after the existing imports (line 7), add:
```rust
use crate::app::LogPaneState;
use crate::logger::{LogCategory, LogSeverity, LogEntry};
```

After the `ERROR_KEY` constant (line 32), add:
```rust
const LOG_KEY: &str = "l: logs";
```

**Step 2: Update help text to include log key**

In `draw()`, update the help text for `ServerList` and `DirectoryBrowser` states to include `LOG_KEY`.

Replace the `ServerList` help text block (lines 41-48) with:
```rust
        AppState::ServerList => {
            if has_errors {
                format!("{} | {} | {} | {} | {} | {} | {}",
                    KEYS.navigate, KEYS.select_server, ERROR_KEY, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit)
            } else {
                format!("{} | {} | {} | {} | {} | {}",
                    KEYS.navigate, KEYS.select_server, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit)
            }
        },
```

Replace the `DirectoryBrowser` help text (line 50-51) with:
```rust
        AppState::DirectoryBrowser => format!("{} | {} | {} | {} | {} | {} | {}",
            KEYS.navigate, KEYS.open, KEYS.back, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit),
```

**Step 3: Update draw() to handle log pane layout**

Replace the main layout section in `draw()` (from line 57 to line 91) with:
```rust
    // Determine if log pane is visible
    let log_visible = app.log_pane_state != LogPaneState::Hidden;
    let log_fullscreen = app.log_pane_state == LogPaneState::Fullscreen;

    if log_fullscreen {
        // Fullscreen log pane
        let [title_area, log_area, help_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(f.area())[..] else { return };

        // Title
        let title = Paragraph::new("MOP - Debug Logs (Fullscreen)")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, title_area);

        draw_log_pane(f, app, log_area);

        let log_help = "l: cycle view | Esc: close | j/k: scroll | t/b: top/bottom | /: filter | s: save";
        let help_paragraph = Paragraph::new(log_help)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(help_paragraph, help_area);
    } else {
        let constraints = if log_visible {
            vec![
                Constraint::Length(3),  // Title
                Constraint::Percentage(65), // Main content
                Constraint::Percentage(35), // Log pane
                Constraint::Length(1),  // Help text
            ]
        } else {
            vec![
                Constraint::Length(3),  // Title
                Constraint::Min(1),     // Main content
                Constraint::Length(1),  // Help text
            ]
        };

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(f.area());

        let title_area = areas[0];
        let content_area = areas[1];
        let (log_area, help_area) = if log_visible {
            (Some(areas[2]), areas[3])
        } else {
            (None, areas[2])
        };

        // Title
        let title = Paragraph::new("MOP - UPnP Device Explorer")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, title_area);

        // Main content area - split horizontally if we have errors
        if has_errors {
            let [main_area, error_area] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(70),
                    Constraint::Percentage(30),
                ])
                .split(content_area)[..] else { return };

            draw_main_content(f, app, main_area);
            draw_error_panel(f, app, error_area);
        } else {
            draw_main_content(f, app, content_area);
        }

        // Log pane
        if let Some(log_area) = log_area {
            draw_log_pane(f, app, log_area);
        }

        // Help text
        let final_help = if log_visible {
            format!("{} | l: cycle view | Esc: close logs", help_text)
        } else {
            help_text
        };
        let help_paragraph = Paragraph::new(final_help)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(help_paragraph, help_area);
    }
```

**Step 4: Add draw_log_pane function**

At the end of `src/ui.rs`, before the closing of the file, add:
```rust

fn draw_log_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let logs = app.get_filtered_logs();
    let total_logs = if let Ok(buffer) = app.log_buffer.lock() {
        buffer.len()
    } else {
        0
    };

    // Calculate visible area (minus borders and footer)
    let visible_height = area.height.saturating_sub(4) as usize; // borders + footer line

    // Clamp scroll offset
    let max_scroll = logs.len().saturating_sub(visible_height);
    if app.log_auto_scroll || app.log_scroll_offset > max_scroll {
        app.log_scroll_offset = max_scroll;
    }

    let visible_logs: Vec<&LogEntry> = logs
        .iter()
        .skip(app.log_scroll_offset)
        .take(visible_height)
        .collect();

    // Split into log content and footer
    let [log_content_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area)[..] else { return };

    // Render log entries
    let log_lines: Vec<Line> = visible_logs
        .iter()
        .map(|entry| {
            let time_span = Span::styled(
                entry.timestamp.format("%H:%M:%S ").to_string(),
                Style::default().fg(Color::DarkGray),
            );

            let category_color = match entry.category {
                LogCategory::Net => Color::Cyan,
                LogCategory::Disc => Color::Green,
                LogCategory::Soap => Color::Magenta,
                LogCategory::Http => Color::Blue,
                LogCategory::Xml => Color::Yellow,
                LogCategory::App => Color::White,
            };

            let (msg_style, cat_style) = match entry.severity {
                LogSeverity::Error => (
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                LogSeverity::Warn => (
                    Style::default().fg(Color::Yellow),
                    Style::default().fg(Color::Yellow),
                ),
                LogSeverity::Info => (
                    Style::default(),
                    Style::default().fg(category_color),
                ),
                LogSeverity::Debug => (
                    Style::default().add_modifier(Modifier::DIM),
                    Style::default().fg(category_color).add_modifier(Modifier::DIM),
                ),
                LogSeverity::Trace => (
                    Style::default().add_modifier(Modifier::DIM).add_modifier(Modifier::ITALIC),
                    Style::default().fg(category_color).add_modifier(Modifier::DIM),
                ),
            };

            let category_span = Span::styled(
                format!("[{}] ", entry.category.as_str()),
                cat_style,
            );

            let message_span = Span::styled(&entry.message, msg_style);

            Line::from(vec![time_span, category_span, message_span])
        })
        .collect();

    let title = if !app.log_filter.is_empty() {
        format!("Logs (showing {} of {})", logs.len(), total_logs)
    } else {
        format!("Logs ({} entries)", logs.len())
    };

    let log_widget = Paragraph::new(log_lines)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(log_widget, log_content_area);

    // Footer with filter
    let footer_content = if app.log_filter_active {
        vec![
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.log_filter_input),
            Span::styled("█", Style::default().fg(Color::White)),
        ]
    } else if !app.log_filter.is_empty() {
        vec![
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::styled(&app.log_filter, Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("[/]filter  [s]ave", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled("[/]filter  [s]ave  [t]op  [b]ottom", Style::default().fg(Color::DarkGray)),
        ]
    };

    let footer = Paragraph::new(Line::from(footer_content))
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, footer_area);
}
```

**Step 5: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 6: Test manually**

Run: `cargo run`
- Press `l` to toggle log pane
- Press `l` again for fullscreen
- Press `l` or `Esc` to close
- Verify layout looks correct

**Step 7: Commit**

```bash
git add src/ui.rs
git commit -m "feat: implement log pane UI rendering"
```

---

## Task 8: Add Instrumentation - Network Module

**Files:**
- Modify: `src/network_interfaces.rs`

**Step 1: Add logging to network interface enumeration**

In `src/network_interfaces.rs`, in `enumerate_network_interfaces()`:

After line 34 (`let interfaces = get_if_addrs()...`), add:
```rust
    log::debug!(target: "mop::net", "Enumerating network interfaces");
```

After line 35 (`.map_err(...)...`), in the Ok case, add:
```rust
    log::debug!(target: "mop::net", "Found {} raw interfaces", interfaces.len());
```

Inside the loop, after adding an interface to result (around line 67), add:
```rust
                log::info!(target: "mop::net", "Found interface {} ({}) multicast={}",
                    interface.name, ip, supports_multicast);
```

Before the final `Ok(result)` (around line 103), add:
```rust
    log::info!(target: "mop::net", "Enumerated {} valid network interfaces", result.len());
```

**Step 2: Add logging to multicast test**

In `test_interface_multicast()` (around line 119), add at the start:
```rust
    log::debug!(target: "mop::net", "Testing multicast capability for {}", interface.name);
```

And change the return statements to log:
```rust
    if interface.is_loopback || !interface.supports_multicast {
        log::debug!(target: "mop::net", "Interface {} skipped: loopback={} multicast={}",
            interface.name, interface.is_loopback, interface.supports_multicast);
        return false;
    }

    match crate::upnp_ssdp::test_multicast_capability() {
        Ok(_) => {
            log::info!(target: "mop::net", "Multicast test passed for {}", interface.name);
            true
        }
        Err(e) => {
            log::warn!(target: "mop::net", "Multicast test failed for {}: {:?}", interface.name, e);
            false
        }
    }
```

**Step 3: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 4: Commit**

```bash
git add src/network_interfaces.rs
git commit -m "feat: add logging to network interface module"
```

---

## Task 9: Add Instrumentation - SSDP Module

**Files:**
- Modify: `src/upnp_ssdp.rs`

**Step 1: Add logging to SsdpDiscovery::new()**

In `src/upnp_ssdp.rs`, in `SsdpDiscovery::new()`:

After socket bind (line 56-63), add on success path:
```rust
        log::info!(target: "mop::net", "SSDP socket bound to 0.0.0.0:0");
```

After setting timeouts (lines 65-67), add:
```rust
        log::debug!(target: "mop::net", "Socket read timeout: 100ms, write timeout: 1000ms");
```

After multicast join (lines 76-82), add on success path (before the Ok):
```rust
        log::info!(target: "mop::net", "Joined multicast group 239.255.255.250 on interface 0.0.0.0");
```

**Step 2: Add logging to discover_devices()**

In `discover_devices()`:

After sending the first M-SEARCH (line 99-105), add:
```rust
        log::info!(target: "mop::ssdp", "Sent M-SEARCH for upnp:rootdevice to 239.255.255.250:1900");
```

After sending the media search (line 114), add:
```rust
        log::info!(target: "mop::ssdp", "Sent M-SEARCH for MediaServer:1 to 239.255.255.250:1900");
```

In the response loop, after successfully parsing a device (line 125-128), add:
```rust
                        if let Some(ref device) = self.parse_ssdp_response(response, addr) {
                            log::debug!(target: "mop::ssdp", "SSDP response from {}: {}", addr, device.location);
```

(Note: you'll need to adjust the if-let pattern slightly)

After the loop ends (before checking if empty, around line 145), add:
```rust
        log::info!(target: "mop::ssdp", "SSDP discovery complete: found {} devices", device_list.len());
```

**Step 3: Add logging to test_multicast_capability()**

In `test_multicast_capability()`:

After socket bind (line 236), add:
```rust
    log::debug!(target: "mop::net", "Multicast test: socket bound");
```

After join_multicast_v4 (line 242), add:
```rust
    log::debug!(target: "mop::net", "Multicast test: joined group 239.255.255.250");
```

After send_to (line 248), add:
```rust
    log::debug!(target: "mop::net", "Multicast test: sent test packet");
```

**Step 4: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 5: Commit**

```bash
git add src/upnp_ssdp.rs
git commit -m "feat: add logging to SSDP discovery module"
```

---

## Task 10: Add Instrumentation - UPnP Module

**Files:**
- Modify: `src/upnp.rs`

**Step 1: Add logging to start_discovery()**

At the start of `discover_with_rupnp()` (after line 42), add:
```rust
    log::info!(target: "mop::upnp", "Starting UPnP discovery (rupnp)");
```

**Step 2: Add logging to rupnp discovery**

Inside the `match rupnp::discover(...)` block:

After entering the Ok branch (line 46), add:
```rust
            log::debug!(target: "mop::upnp", "rupnp discovery stream started, timeout=5s");
```

When a device is found (around line 54), add:
```rust
                    log::info!(target: "mop::upnp", "Device found: {} ({})", friendly_name, device_type);
                    log::debug!(target: "mop::upnp", "  URL: {}", device_url);
```

In the Err branch (line 104-106), the error is already sent via channel, but add:
```rust
            log::error!(target: "mop::upnp", "rupnp discovery failed: {}", e);
```

**Step 3: Add logging to phase transitions**

After Phase1Complete (line 109), add:
```rust
    log::info!(target: "mop::upnp", "Phase 1 complete: found {} devices", devices.len());
```

After Phase2Complete (line 113), add:
```rust
    log::debug!(target: "mop::upnp", "Phase 2 complete (no-op)");
```

After Phase3Complete (line 130), add:
```rust
    log::info!(target: "mop::upnp", "Phase 3 (port scan) complete");
```

**Step 4: Add logging to port scan**

In `targeted_port_scan()`:

At the start (around line 135), add:
```rust
    log::debug!(target: "mop::upnp", "Starting targeted port scan");
```

After getting network base (line 138-141), add:
```rust
    log::debug!(target: "mop::upnp", "Scanning network base: {}", network_base);
```

In `scan_single_endpoint()`:

At the start (line 159), add:
```rust
    log::trace!(target: "mop::http", "Probing {}:{}", ip, port);
```

On success (before returning Some, around line 180), add:
```rust
                log::info!(target: "mop::upnp", "Found server via port scan: {}:{}", ip, port);
```

**Step 5: Add logging to fetch_device_description()**

At the start (line 194), add:
```rust
    log::debug!(target: "mop::http", "Fetching device description: {}", device_url);
```

On success, add:
```rust
    log::debug!(target: "mop::xml", "Received device description: {} bytes", response.text().await?.len());
```

(Note: you'll need to refactor slightly to log before consuming the response)

**Step 6: Add logging to SOAP browsing**

In `browse_upnp_content_directory_with_id()` (you'll need to find this function), add:
```rust
    log::debug!(target: "mop::soap", "SOAP Browse request to {} (container={})", content_dir_url, container_id);
```

On response:
```rust
    log::debug!(target: "mop::soap", "SOAP response: {} status", response.status());
```

**Step 7: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 8: Commit**

```bash
git add src/upnp.rs
git commit -m "feat: add logging to UPnP discovery module"
```

---

## Task 11: Add Instrumentation - App Events

**Files:**
- Modify: `src/app.rs`

**Step 1: Add logging to discovery lifecycle**

In `start_discovery()` (around line 92), add at the start:
```rust
        log::info!(target: "mop::app", "Starting device discovery");
```

In `check_discovery_updates()`, add logging for each message type:

For `Started`:
```rust
                    DiscoveryMessage::Started => {
                        log::info!(target: "mop::app", "Discovery started");
                        self.is_discovering = true;
                        self.discovery_errors.clear();
                    }
```

For `DeviceFound`:
```rust
                    DiscoveryMessage::DeviceFound(device) => {
                        log::info!(target: "mop::app", "Device added to list: {}", device.name);
                        // ... rest of existing code
                    }
```

For `AllComplete`:
```rust
                    DiscoveryMessage::AllComplete(final_devices) => {
                        log::info!(target: "mop::app", "Discovery complete: {} total devices", final_devices.len());
                        // ... rest of existing code
                    }
```

For `Error`:
```rust
                    DiscoveryMessage::Error(error) => {
                        log::error!(target: "mop::app", "Discovery error: {}", error);
                        // ... rest of existing code
                    }
```

**Step 2: Add logging to navigation**

In `select()`, for server selection:
```rust
                        log::info!(target: "mop::app", "Selected server: {}", self.servers[server_idx].name);
```

In `load_directory()`:
```rust
        log::debug!(target: "mop::app", "Loading directory: /{}", self.current_directory.join("/"));
```

In `play_selected_file()`, on success:
```rust
                        log::info!(target: "mop::app", "Playing file: {}", item.name);
```

**Step 3: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat: add logging to app events"
```

---

## Task 12: Update Help Modal

**Files:**
- Modify: `src/ui.rs`

**Step 1: Add log key to help modal**

In `draw_help_modal()`, increase `modal_height` to 20 (line 433).

In the help_text vec, after the "Actions:" section, add:
```rust
        Line::from(LOG_KEY),
```

**Step 2: Verify build**

Run: `cargo build 2>&1 | grep -E "error"`
Expected: No errors

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add log key to help modal"
```

---

## Task 13: Final Testing and Cleanup

**Files:**
- All modified files

**Step 1: Run full build**

Run: `cargo build --release 2>&1`
Expected: Build succeeds

**Step 2: Run clippy**

Run: `cargo clippy 2>&1 | grep -E "error|warning:" | head -30`
Fix any new warnings introduced by this feature.

**Step 3: Run formatter**

Run: `cargo fmt`

**Step 4: Manual testing**

Run: `cargo run`

Test checklist:
- [ ] Press `l` - bottom log pane appears (35% height)
- [ ] Press `l` again - fullscreen log pane
- [ ] Press `l` again - log pane closes
- [ ] Press `Esc` while log pane open - closes log pane
- [ ] Press `j`/`k` or arrows - scrolls logs
- [ ] Press `t` - jumps to top
- [ ] Press `b` - jumps to bottom
- [ ] Press `/` - filter input appears
- [ ] Type filter text, press Enter - filters applied
- [ ] Press `s` - exports logs to ~/.cache/mop/
- [ ] Verify logs show discovery phases
- [ ] Verify category colors (NET=cyan, DISC=green, etc.)
- [ ] Verify errors show in red

**Step 5: Commit any fixes**

```bash
git add -A
git commit -m "chore: final cleanup and formatting"
```

---

## Summary

Tasks completed:
1. Added log + chrono dependencies
2. Created logger module with data types
3. Implemented RingBufferLogger
4. Initialized logger in main
5. Added log pane state to App
6. Added log pane key bindings
7. Implemented log pane UI
8. Added instrumentation to network module
9. Added instrumentation to SSDP module
10. Added instrumentation to UPnP module
11. Added instrumentation to App events
12. Updated help modal
13. Final testing and cleanup

The log pane is now fully functional for debugging UPnP discovery issues.
