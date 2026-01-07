use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, AppState, LogPaneState};
use crate::logger::{LogCategory, LogSeverity, LogEntry};

struct KeyMappings {
    navigate: &'static str,
    select_server: &'static str,
    open: &'static str,
    back: &'static str,
    back_to_directory: &'static str,
    help: &'static str,
    quit: &'static str,
}

const KEYS: KeyMappings = KeyMappings {
    navigate: "↑↓: navigate",
    select_server: "enter: select server",
    open: "enter: play/open",
    back: "backspace: back",
    back_to_directory: "enter: back to directory",
    help: "?: help",
    quit: "q: quit",
};

const ERROR_KEY: &str = "e: dump errors";
const CONFIG_KEY: &str = "c: config";
const LOG_KEY: &str = "l: logs";


pub fn draw(f: &mut Frame, app: &mut App) {
    // Check if we have errors to show
    let has_errors = app.last_error.is_some() || !app.discovery_errors.is_empty();

    // Get help text based on current state
    let help_text = match app.state {
        AppState::ServerList => {
            if has_errors {
                format!("{} | {} | {} | {} | {} | {} | {}",
                    KEYS.navigate, KEYS.select_server, ERROR_KEY, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit)
            } else {
                format!("{} | {} | {} | {} | {} | {}",
                    KEYS.navigate, KEYS.select_server, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit)
            }
        },
        AppState::DirectoryBrowser => format!("{} | {} | {} | {} | {} | {} | {}",
            KEYS.navigate, KEYS.open, KEYS.back, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit),
        AppState::FileDetails => format!("{} | {} | {} | {} | {}",
            KEYS.back_to_directory, LOG_KEY, CONFIG_KEY, KEYS.help, KEYS.quit),
    };

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

    // Draw help modal if shown
    if app.show_help {
        draw_help_modal(f);
    }

    // Draw config modal if shown
    if app.show_config {
        draw_config_modal(f, app);
    }
}

fn draw_file_info_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut info_lines = Vec::new();
    
    if let Some(item_idx) = app.selected_item {
        if item_idx < app.directory_contents.len() {
            let item = &app.directory_contents[item_idx];
            
            info_lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::raw(&item.name),
            ]));
            
            info_lines.push(Line::from(vec![
                Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                Span::raw(if item.is_directory { "Directory" } else { "File" }),
            ]));
            
            if let Some(url) = &item.url {
                info_lines.push(Line::from(""));
                info_lines.push(Line::from(vec![
                    Span::styled("URL: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]));
                // Split long URLs into multiple lines
                let url_lines = wrap_text(url, area.width.saturating_sub(4) as usize);
                for line in url_lines {
                    info_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::raw(line),
                    ]));
                }
            }
            
            if let Some(metadata) = &item.metadata {
                info_lines.push(Line::from(""));
                info_lines.push(Line::from(vec![
                    Span::styled("Metadata:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]));
                
                if let Some(size) = metadata.size {
                    info_lines.push(Line::from(vec![
                        Span::raw("  Size: "),
                        Span::raw(format_size(size)),
                    ]));
                }
                
                if let Some(duration) = &metadata.duration {
                    info_lines.push(Line::from(vec![
                        Span::raw("  Duration: "),
                        Span::raw(duration),
                    ]));
                }
                
                if let Some(format) = &metadata.format {
                    info_lines.push(Line::from(vec![
                        Span::raw("  Format: "),
                        Span::raw(format),
                    ]));
                }
            }
        }
    } else {
        info_lines.push(Line::from(vec![
            Span::styled("No item selected", Style::default().fg(Color::Gray)),
        ]));
    }
    
    let info = Paragraph::new(info_lines)
        .block(Block::default().borders(Borders::ALL).title("File Info"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(info, area);
}

fn draw_server_info_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut info_lines = Vec::new();
    
    if let Some(server_idx) = app.selected_server {
        if server_idx < app.servers.len() {
            let server = &app.servers[server_idx];
            
            info_lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::raw(&server.name),
            ]));
            
            info_lines.push(Line::from(""));
            
            info_lines.push(Line::from(vec![
                Span::styled("Location: ", Style::default().fg(Color::Green)),
            ]));
            // Split long URLs into multiple lines
            let url_lines = wrap_text(&server.location, area.width.saturating_sub(4) as usize);
            for line in url_lines {
                info_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(line),
                ]));
            }
            
            info_lines.push(Line::from(""));
            
            info_lines.push(Line::from(vec![
                Span::styled("Base URL: ", Style::default().fg(Color::Green)),
            ]));
            let base_url_lines = wrap_text(&server.base_url, area.width.saturating_sub(4) as usize);
            for line in base_url_lines {
                info_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(line),
                ]));
            }
            
            if let Some(content_url) = &server.content_directory_url {
                info_lines.push(Line::from(""));
                info_lines.push(Line::from(vec![
                    Span::styled("Content Directory: ", Style::default().fg(Color::Yellow)),
                ]));
                let content_lines = wrap_text(content_url, area.width.saturating_sub(4) as usize);
                for line in content_lines {
                    info_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::raw(line),
                    ]));
                }
            }
        }
    } else {
        info_lines.push(Line::from(vec![
            Span::styled("No server selected", Style::default().fg(Color::Gray)),
        ]));
    }
    
    let info = Paragraph::new(info_lines)
        .block(Block::default().borders(Borders::ALL).title("Server Info"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(info, area);
}

fn draw_error_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut error_lines = Vec::new();
    
    if !app.discovery_errors.is_empty() {
        // Show ALL errors with numbering for easy selection
        for (i, error) in app.discovery_errors.iter().enumerate() {
            error_lines.push(Line::from(vec![
                Span::styled(format!("{}. ", i + 1), Style::default().fg(Color::Yellow)),
                Span::raw(error),
            ]));
        }
        
        error_lines.push(Line::from(""));
        error_lines.push(Line::from(vec![
            Span::styled("Press 'e' to copy", Style::default().fg(Color::Cyan)),
        ]));
    }
    
    let errors = Paragraph::new(error_lines)
        .block(Block::default().borders(Borders::ALL).title("Errors"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(errors, area);
}


fn draw_main_content(f: &mut Frame, app: &App, area: Rect) {
    match app.state {
        AppState::ServerList => {
            // Split area into server list and server info panel
            let [list_area, info_area] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(60),  // Server list
                    Constraint::Percentage(40),  // Server info panel
                ])
                .split(area)[..] else { return };

            let items: Vec<ListItem> = app
                .servers
                .iter()
                .enumerate()
                .map(|(i, server)| {
                    let style = if Some(i) == app.selected_server {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    
                    // Extract clean device name (remove bracketed info)
                    let clean_name = if let Some(bracket_pos) = server.name.find(" [") {
                        &server.name[..bracket_pos]
                    } else {
                        &server.name
                    };
                    
                    ListItem::new(Line::from(vec![
                        Span::styled(clean_name, style),
                    ]))
                })
                .collect();

            let title = if app.is_discovering {
                "[•] Discovered UPnP Devices"
            } else {
                "[ ] Discovered UPnP Devices"
            };

            let list = List::new(items)
                .block(Block::default()
                    .title(title)
                    .borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut list_state = ListState::default();
            list_state.select(app.selected_server);
            
            f.render_stateful_widget(list, list_area, &mut list_state);
            
            // Draw server info panel
            draw_server_info_panel(f, app, info_area);
        },
        AppState::DirectoryBrowser => {
            let current_path = if app.current_directory.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", app.current_directory.join("/"))
            };

            // Split area into directory list and file info panel
            let [list_area, info_area] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(60),  // Directory list
                    Constraint::Percentage(40),  // File info panel
                ])
                .split(area)[..] else { return };

            let items: Vec<ListItem> = app
                .directory_contents
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let style = if Some(i) == app.selected_item {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    
                    let icon = if item.is_directory { "📁" } else { "📄" };
                    
                    ListItem::new(Line::from(vec![
                        Span::raw(icon),
                        Span::raw(" "),
                        Span::styled(&item.name, style),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default()
                    .title(format!("Directory: {}", current_path))
                    .borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut list_state = ListState::default();
            list_state.select(app.selected_item);
            
            f.render_stateful_widget(list, list_area, &mut list_state);
            
            // Draw file info panel
            draw_file_info_panel(f, app, info_area);
        },
        AppState::FileDetails => {
            if let Some(item_idx) = app.selected_item {
                if item_idx < app.directory_contents.len() {
                    let item = &app.directory_contents[item_idx];
                    
                    let mut details = vec![
                        Line::from(vec![
                            Span::styled("File: ", Style::default().fg(Color::Cyan)),
                            Span::raw(&item.name),
                        ]),
                    ];

                    if let Some(url) = &item.url {
                        details.push(Line::from(vec![
                            Span::styled("Direct URL: ", Style::default().fg(Color::Green)),
                            Span::raw(url),
                        ]));
                    }

                    if let Some(metadata) = &item.metadata {
                        if let Some(size) = metadata.size {
                            details.push(Line::from(vec![
                                Span::styled("Size: ", Style::default().fg(Color::Yellow)),
                                Span::raw(format_size(size)),
                            ]));
                        }
                        
                        if let Some(duration) = &metadata.duration {
                            details.push(Line::from(vec![
                                Span::styled("Duration: ", Style::default().fg(Color::Yellow)),
                                Span::raw(duration),
                            ]));
                        }
                        
                        if let Some(format) = &metadata.format {
                            details.push(Line::from(vec![
                                Span::styled("Format: ", Style::default().fg(Color::Yellow)),
                                Span::raw(format),
                            ]));
                        }
                    }

                    let paragraph = Paragraph::new(details)
                        .block(Block::default()
                            .title("File Details")
                            .borders(Borders::ALL));
                    
                    f.render_widget(paragraph, area);
                }
            }
        }

    }
}

fn draw_help_modal(f: &mut Frame) {
    let area = f.area();
    
    // Calculate centered modal size - make it bigger for more keys
    let modal_width = 65;
    let modal_height = 18;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    
    let modal_area = Rect {
        x,
        y,
        width: modal_width,
        height: modal_height,
    };
    
    // Clear the background
    f.render_widget(Clear, modal_area);
    
    let help_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("MOP - UPnP Device Explorer", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from("Vibecoded for Omarchy: discover UPnP devices and"),
        Line::from("browse media content directly. Press Enter on"),
        Line::from("files to play them with mpv."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(KEYS.navigate),
        Line::from(KEYS.select_server),
        Line::from(KEYS.open),
        Line::from(KEYS.back),
        Line::from(""),
        Line::from(vec![
            Span::styled("Actions:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(CONFIG_KEY),
        Line::from(ERROR_KEY),
        Line::from(KEYS.help),
        Line::from(KEYS.quit),
        Line::from(""),
    ];
    
    let paragraph = Paragraph::new(help_text)
        .block(Block::default()
            .title("Help")
            .title_bottom("Press ? or Esc to close")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)))
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, modal_area);
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.len() <= max_width {
        return vec![text.to_string()];
    }
    
    let mut lines = Vec::new();
    let mut current_line = String::new();
    
    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    
    lines
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

fn draw_config_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    
    // Calculate centered modal size - simpler and smaller
    let modal_width = 70;
    let modal_height = 12;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    
    let modal_area = Rect {
        x,
        y,
        width: modal_width,
        height: modal_height,
    };
    
    // Clear just the modal area for clean overlay
    f.render_widget(Clear, modal_area);
    let block = Block::default()
        .title("Configuration")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    
    // Get inner area
    let inner_area = block.inner(modal_area);
    f.render_widget(block, modal_area);
    
    // Split into content and help
    let [content_area, help_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Content
            Constraint::Min(1),     // Help
        ])
        .split(inner_area)[..] else { return };

    // Simple vertical layout for fields
    let [input_line, checkbox_line, spacing] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Input with border
            Constraint::Length(1),  // Checkbox line
            Constraint::Length(2),  // Spacing
        ])
        .split(content_area)[..] else { return };
    
    // Media player command input
    let run_border_style = if app.config_editor.selected_field == crate::app::ConfigField::Run {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let run_input = Paragraph::new(app.config_editor.run_input.value())
        .block(Block::default()
            .title("Media Player Command")
            .borders(Borders::ALL)
            .border_style(run_border_style));
    f.render_widget(run_input, input_line);
    
    // Simple checkbox line - DOS/MC style
    let checkbox_symbol = if app.config_editor.auto_close { "[x]" } else { "[ ]" };
    let checkbox_style = if app.config_editor.selected_field == crate::app::ConfigField::AutoClose {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let checkbox_text = format!("{} Auto close after launch", checkbox_symbol);
    let checkbox_para = Paragraph::new(checkbox_text)
        .style(checkbox_style);
    f.render_widget(checkbox_para, checkbox_line);
    
    // Simple help text
    let help_text = "Tab/Shift+Tab: Navigate | Space: Toggle | Enter: Save | Esc: Cancel";
    let help_para = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(help_para, help_area);
    
    // Position cursor
    if app.config_editor.selected_field == crate::app::ConfigField::Run {
        f.set_cursor_position((
            input_line.x + app.config_editor.run_input.cursor() as u16 + 1,
            input_line.y + 1,
        ));
    }
}

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