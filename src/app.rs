use crate::upnp::{PlexServer, DiscoveryMessage};
use crate::config::Config;
use std::sync::mpsc::Receiver;
use std::collections::HashMap;
use tui_input::Input;


#[derive(Debug, Clone)]
pub enum AppState {
    ServerList,
    DirectoryBrowser,
    FileDetails,
}

pub struct App {
    pub state: AppState,
    pub servers: Vec<PlexServer>,
    pub selected_server: Option<usize>,
    pub current_directory: Vec<String>,
    pub directory_contents: Vec<DirectoryItem>,
    pub selected_item: Option<usize>,
    pub status_message: String,
    pub last_error: Option<String>,
    pub discovery_errors: Vec<String>,
    discovery_receiver: Option<Receiver<DiscoveryMessage>>,
    pub is_discovering: bool,
    pub show_help: bool,
    pub show_config: bool,
    pub should_quit: bool,
    pub container_id_map: HashMap<Vec<String>, String>,
    pub config: Config,
    pub config_editor: ConfigEditor,
}

pub struct ConfigEditor {
    pub run_input: Input,
    pub auto_close: bool,
    pub selected_field: ConfigField,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigField {
    Run,
    AutoClose,
}

#[derive(Debug, Clone)]
pub struct DirectoryItem {
    pub name: String,
    pub is_directory: bool,
    pub url: Option<String>,
    pub metadata: Option<FileMetadata>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: Option<u64>,
    pub duration: Option<String>,
    pub format: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let config = Config::load();
        let config_editor = ConfigEditor::new(&config);
        
        let mut app = Self {
            state: AppState::ServerList,
            servers: Vec::new(),
            selected_server: None,
            current_directory: Vec::new(),
            directory_contents: Vec::new(),
            selected_item: None,
            status_message: "".to_string(),
            last_error: None,
            discovery_errors: Vec::new(),
            discovery_receiver: None,
            is_discovering: false,
            show_help: false,
            show_config: false,
            should_quit: false,
            container_id_map: HashMap::new(),
            config,
            config_editor,
        };
        
        // Initialize with root container ID
        app.container_id_map.insert(Vec::new(), "0".to_string());
        app
    }
    
    pub fn start_discovery(&mut self) {
        // Don't start if already running
        if self.discovery_receiver.is_some() {
            return;
        }
        
        // Use the new simplified discovery system
        let receiver = crate::upnp::start_discovery();
        self.discovery_receiver = Some(receiver);
        self.is_discovering = true;
    }
    
    pub fn check_discovery_updates(&mut self) {
        let mut should_clear_receiver = false;
        
        if let Some(ref receiver) = self.discovery_receiver {
            while let Ok(message) = receiver.try_recv() {
                match message {
                    DiscoveryMessage::Started => {
                        self.is_discovering = true;
                        self.discovery_errors.clear();
                    }
                    DiscoveryMessage::DeviceFound(device) => {
                        // Add device immediately for responsive UI with proper deduplication
                        if !self.servers.iter().any(|d| d.location == device.location) {
                            self.servers.push(device);
                        }
                    }
                    DiscoveryMessage::Phase1Complete => {
                        // SSDP discovery phase complete
                    }
                    DiscoveryMessage::Phase2Complete => {
                        // Extended discovery phase complete
                    }
                    DiscoveryMessage::Phase3Complete => {
                        // Port scan phase complete
                    }
                    DiscoveryMessage::AllComplete(final_devices) => {
                        // Merge final devices with existing ones, avoiding duplicates
                        for device in final_devices {
                            if !self.servers.iter().any(|d| d.location == device.location) {
                                self.servers.push(device);
                            }
                        }
                        self.is_discovering = false;
                        should_clear_receiver = true;
                        
                        if self.servers.is_empty() {
                            self.last_error = Some("No UPnP devices found".to_string());
                        } else {
                            self.last_error = None;
                        }
                    }
                    DiscoveryMessage::Error(error) => {
                        self.discovery_errors.push(error.clone());
                        // Always show the latest error
                        self.last_error = Some(error);
                        // Don't stop discovery on individual errors - continue until AllComplete
                    }
                }
            }
        }
        
        if should_clear_receiver {
            self.discovery_receiver = None;
        }
    }

    pub fn refresh_servers(&mut self) {
        // Clear existing state and restart discovery
        self.servers.clear();
        self.discovery_errors.clear();
        self.last_error = None;
        self.discovery_receiver = None;
        self.is_discovering = false;
        self.start_discovery();
    }

    pub fn previous(&mut self) {
        match self.state {
            AppState::ServerList => {
                if !self.servers.is_empty() {
                    self.selected_server = match self.selected_server {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(_) => Some(self.servers.len() - 1),
                        None => Some(0),
                    };
                }
            },
            AppState::DirectoryBrowser => {
                if !self.directory_contents.is_empty() {
                    self.selected_item = match self.selected_item {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(_) => Some(self.directory_contents.len() - 1),
                        None => Some(0),
                    };
                }
            },
            _ => {}
        }
    }

    pub fn next(&mut self) {
        match self.state {
            AppState::ServerList => {
                if !self.servers.is_empty() {
                    self.selected_server = match self.selected_server {
                        Some(i) if i < self.servers.len() - 1 => Some(i + 1),
                        Some(_) => Some(0),
                        None => Some(0),
                    };
                }
            },
            AppState::DirectoryBrowser => {
                if !self.directory_contents.is_empty() {
                    self.selected_item = match self.selected_item {
                        Some(i) if i < self.directory_contents.len() - 1 => Some(i + 1),
                        Some(_) => Some(0),
                        None => Some(0),
                    };
                }
            },
            _ => {}
        }
    }

    pub fn select(&mut self) {
        match self.state {
            AppState::ServerList => {
                if let Some(server_idx) = self.selected_server {
                    if server_idx < self.servers.len() {
                        self.state = AppState::DirectoryBrowser;
                        self.current_directory.clear();
                        self.load_directory();
                    }
                }
            },
            AppState::DirectoryBrowser => {
                if let Some(item_idx) = self.selected_item {
                    if item_idx < self.directory_contents.len() {
                        let item = &self.directory_contents[item_idx];
                        if item.is_directory {
                            self.current_directory.push(item.name.clone());
                            self.load_directory();
                        } else {
                            // For files, try to play with mpv
                            match self.play_selected_file() {
                                Ok(_) => {
                                    // mpv started successfully, clear any previous errors
                                    self.last_error = None;
                                }
                                Err(e) => {
                                    // mpv failed, show error
                                    self.last_error = Some(format!("Failed to play file: {}", e));
                                }
                            }
                        }
                    }
                }
            },
            AppState::FileDetails => {
                self.state = AppState::DirectoryBrowser;
            }

        }
    }

    pub fn go_back(&mut self) {
        match self.state {
            AppState::DirectoryBrowser => {
                if self.current_directory.is_empty() {
                    self.state = AppState::ServerList;
                } else {
                    self.current_directory.pop();
                    self.load_directory();
                }
            },
            AppState::FileDetails => {
                self.state = AppState::DirectoryBrowser;
            },
            _ => {}
        }
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    fn load_directory(&mut self) {
        if let Some(server_idx) = self.selected_server {
            if server_idx < self.servers.len() {
                let server = &self.servers[server_idx];
                let (contents, error) = crate::upnp::browse_directory(server, &self.current_directory, &mut self.container_id_map);
                self.directory_contents = contents;
                self.last_error = error;
                self.selected_item = if self.directory_contents.is_empty() { None } else { Some(0) };
            }
        }
    }

    pub fn get_selected_file_url(&self) -> Option<String> {
        if let AppState::FileDetails = self.state {
            if let Some(item_idx) = self.selected_item {
                if item_idx < self.directory_contents.len() {
                    return self.directory_contents[item_idx].url.clone();
                }
            }
        }
        None
    }

    pub fn play_selected_file(&mut self) -> Result<(), String> {
        if let Some(item_idx) = self.selected_item {
            if item_idx < self.directory_contents.len() {
                let item = &self.directory_contents[item_idx];
                if !item.is_directory {
                    if let Some(url) = &item.url {
                        let result = self.invoke_player(url);
                        if result.is_ok() && self.config.mop.auto_close {
                            self.should_quit = true;
                        }
                        return result;
                    } else {
                        return Err("No URL available for this file".to_string());
                    }
                } else {
                    return Err("Cannot play a directory".to_string());
                }
            }
        }
        Err("No file selected".to_string())
    }

    fn invoke_mpv(&self, url: &str) -> Result<(), String> {
        self.invoke_player(url)
    }

    fn invoke_player(&self, url: &str) -> Result<(), String> {
        use std::process::Command;
        
        let player = &self.config.mop.run;
        
        // Use setsid with nohup for complete session detachment
        // This ensures the player runs completely independently of MOP
        let cmd_str = format!("setsid nohup {} '{}' </dev/null >/dev/null 2>&1 &", player, url);
        let status = Command::new("sh")
            .arg("-c")
            .arg(&cmd_str)
            .status()
            .map_err(|e| format!("Failed to start {}: {}", player, e))?;
        
        if status.success() {
            Ok(())
        } else {
            Err(format!("Failed to start {} command", player))
        }
    }
    
    fn get_container_id(&self, path: &[String]) -> String {
        if path.is_empty() {
            "0".to_string() // Root container
        } else {
            self.container_id_map.get(path).cloned().unwrap_or_else(|| {
                // This should not happen in correct implementation
                "0".to_string()
            })
        }
    }
    
    fn set_container_id(&mut self, path: &[String], container_id: String) {
        self.container_id_map.insert(path.to_vec(), container_id);
    }

    pub fn open_config_editor(&mut self) {
        self.show_config = true;
        self.config_editor = ConfigEditor::new(&self.config);
    }

    pub fn save_config(&mut self) -> Result<(), String> {
        // Update config from editor
        self.config.mop.run = self.config_editor.run_input.value().to_string();
        self.config.mop.auto_close = self.config_editor.auto_close;
        
        // Save to file
        match self.config.save() {
            Ok(_) => {
                self.show_config = false;
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                let error = format!("Failed to save config: {}", e);
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn cancel_config_edit(&mut self) {
        self.show_config = false;
        self.config_editor = ConfigEditor::new(&self.config);
    }
}

impl ConfigEditor {
    pub fn new(config: &Config) -> Self {
        let mut run_input = Input::default();
        run_input = run_input.with_value(config.mop.run.clone());
        
        Self {
            run_input,
            auto_close: config.mop.auto_close,
            selected_field: ConfigField::Run,
        }
    }

    pub fn next_field(&mut self) {
        self.selected_field = match self.selected_field {
            ConfigField::Run => ConfigField::AutoClose,
            ConfigField::AutoClose => ConfigField::Run,
        };
    }

    pub fn previous_field(&mut self) {
        self.selected_field = match self.selected_field {
            ConfigField::Run => ConfigField::AutoClose,
            ConfigField::AutoClose => ConfigField::Run,
        };
    }

    pub fn toggle_auto_close(&mut self) {
        if self.selected_field == ConfigField::AutoClose {
            self.auto_close = !self.auto_close;
        }
    }

    pub fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) -> bool {
        use ratatui::crossterm::event::{KeyCode, Event};
        use tui_input::backend::crossterm::EventHandler;
        
        match self.selected_field {
            ConfigField::Run => {
                // Convert KeyEvent to Event for tui-input
                let event = Event::Key(key);
                self.run_input.handle_event(&event);
                true
            }
            ConfigField::AutoClose => {
                match key.code {
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        self.toggle_auto_close();
                        true
                    }
                    _ => false
                }
            }
        }
    }
}