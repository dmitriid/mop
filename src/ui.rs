use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, AppState};

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
    open: "enter: open",
    back: "backspace: back",
    back_to_directory: "enter: back to directory",
    help: "?: help",
    quit: "q: quit",
};

const ERROR_KEY: &str = "e: dump errors";


pub fn draw(f: &mut Frame, app: &mut App) {
    // Check if we have errors to show
    let has_errors = app.last_error.is_some() || !app.discovery_errors.is_empty();
    
    // Get help text based on current state
    let help_text = match app.state {
        AppState::ServerList => {
            if has_errors {
                format!("─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────", 
                    KEYS.navigate, KEYS.select_server, ERROR_KEY, KEYS.help, KEYS.quit)
            } else {
                format!("─────| {} |─────| {} |─────| {} |─────| {} |─────", 
                    KEYS.navigate, KEYS.select_server, KEYS.help, KEYS.quit)
            }
        },
        AppState::DirectoryBrowser => format!("─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────", 
            KEYS.navigate, KEYS.open, KEYS.back, KEYS.help, KEYS.quit),
        AppState::FileDetails => format!("─────| {} |─────| {} |─────| {} |─────", 
            KEYS.back_to_directory, KEYS.help, KEYS.quit),
    };
    
    let [title_area, content_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(1),     // Main content
        ])
        .split(f.area())[..] else { return };

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
                Constraint::Percentage(70),  // Main content
                Constraint::Percentage(30),  // Errors
            ])
            .split(content_area)[..] else { return };
            
        draw_main_content(f, app, main_area, &help_text);
        draw_error_panel(f, app, error_area);
    } else {
        draw_main_content(f, app, content_area, &help_text);
    }

    // Draw help modal if shown
    if app.show_help {
        draw_help_modal(f);
    }
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


fn draw_main_content(f: &mut Frame, app: &App, area: Rect, help_text: &str) {
    match app.state {
        AppState::ServerList => {
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
                    
                    ListItem::new(Line::from(vec![
                        Span::styled(&server.name, style),
                        Span::raw(" - "),
                        Span::styled(&server.location, Style::default().fg(Color::Gray)),
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
                    .title_bottom(help_text)
                    .borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut list_state = ListState::default();
            list_state.select(app.selected_server);
            
            f.render_stateful_widget(list, area, &mut list_state);
        },
        AppState::DirectoryBrowser => {
            let current_path = if app.current_directory.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", app.current_directory.join("/"))
            };

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
                    .title_bottom(help_text)
                    .borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut list_state = ListState::default();
            list_state.select(app.selected_item);
            
            f.render_stateful_widget(list, area, &mut list_state);
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
                            .title_bottom(help_text)
                            .borders(Borders::ALL));
                    
                    f.render_widget(paragraph, area);
                }
            }
        }
    }
}

fn draw_help_modal(f: &mut Frame) {
    let area = f.area();
    
    // Calculate centered modal size
    let modal_width = 60;
    let modal_height = 14;
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
        Line::from("browse media content directly."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Keys:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(KEYS.navigate),
        Line::from(KEYS.select_server),
        Line::from(KEYS.open),
        Line::from(KEYS.back),
        Line::from(KEYS.help),
        Line::from(KEYS.quit),
        Line::from(""),
    ];
    
    let paragraph = Paragraph::new(help_text)
        .block(Block::default()
            .title("Help")
            .title_bottom("Press ? to close")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)))
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, modal_area);
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