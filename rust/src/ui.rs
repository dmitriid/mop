use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget},
    Frame,
};
use ratatui_input::Input;

use crate::app::{App, AppState, SettingsField};

struct KeyMappings {
    navigate: &'static str,
    select_server: &'static str,
    open: &'static str,
    back: &'static str,
    back_to_directory: &'static str,
    help: &'static str,
    settings: &'static str,
    quit: &'static str,
}

const KEYS: KeyMappings = KeyMappings {
    navigate: "↑↓: navigate",
    select_server: "enter: select server",
    open: "enter: play/open",
    back: "backspace: back",
    back_to_directory: "enter: back to directory",
    help: "?: help",
    settings: ",: settings",
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
                format!("─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────", 
                    KEYS.navigate, KEYS.select_server, ERROR_KEY, KEYS.help, KEYS.settings, KEYS.quit)
            } else {
                format!("─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────", 
                    KEYS.navigate, KEYS.select_server, KEYS.help, KEYS.settings, KEYS.quit)
            }
        },
        AppState::DirectoryBrowser => format!("─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────| {} |─────", 
            KEYS.navigate, KEYS.open, KEYS.back, KEYS.help, KEYS.settings, KEYS.quit),
        AppState::FileDetails => format!("─────| {} |─────| {} |─────| {} |─────| {} |─────", 
            KEYS.back_to_directory, KEYS.help, KEYS.settings, KEYS.quit),
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
    
    // Draw settings modal if shown
    if app.show_settings {
        draw_settings_modal(f, app);
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
                    .title_bottom(help_text)
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
        Line::from("browse media content directly. Press Enter on"),
        Line::from("files to play them with mpv."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Keys:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(KEYS.navigate),
        Line::from(KEYS.select_server),
        Line::from(KEYS.open),
        Line::from(KEYS.back),
        Line::from(KEYS.help),
        Line::from(KEYS.settings),
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

fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    
    // Calculate centered modal size
    let modal_width = 60;
    let modal_height = 16;
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
    
    if app.settings_editing {
        // When editing, show the input widget
        let input_widget = Input::default();
        // Use the render method from ratatui_input which is compatible with ratatui 0.26.3
        <Input as StatefulWidget>::render(input_widget, modal_area, f.buffer_mut(), &mut app.settings_input_state);
    } else {
        // When not editing, show the settings overview
        let mut settings_text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Settings", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];
        
        // Player field
        let player_style = if app.settings_field == SettingsField::Player {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        
        settings_text.push(Line::from(vec![
            Span::styled("Player: ", player_style),
            Span::raw(&app.config.mop.run),
        ]));
        
        // Close on run field
        let close_style = if app.settings_field == SettingsField::CloseOnRun {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        
        let close_value = if app.config.mop.close_on_run { "Yes" } else { "No" };
        
        settings_text.push(Line::from(vec![
            Span::styled("Close on run: ", close_style),
            Span::raw(close_value),
        ]));
        
        settings_text.push(Line::from(""));
        settings_text.push(Line::from(vec![
            Span::styled("Config file: ", Style::default().fg(Color::Gray)),
        ]));
        settings_text.push(Line::from(vec![
            Span::raw("  ~/.config/mop.toml"),
        ]));
        settings_text.push(Line::from(""));
        
        // Instructions
        settings_text.push(Line::from(vec![
            Span::styled("Navigation: ", Style::default().fg(Color::Cyan)),
        ]));
        settings_text.push(Line::from("  e: edit, Tab: next field, ,: close"));
        
        let paragraph = Paragraph::new(settings_text)
            .block(Block::default()
                .title("Settings")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Black)))
            .alignment(Alignment::Left);
        
        f.render_widget(paragraph, modal_area);
    }
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