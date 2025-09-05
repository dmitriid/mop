use crate::upnp::{PlexServer, DiscoveryMessage};
use std::sync::mpsc::Receiver;

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
        Self {
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
        }
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
                        // Add device immediately for responsive UI
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
                        self.servers = final_devices;
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
                            self.state = AppState::FileDetails;
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
                let (contents, error) = crate::upnp::browse_directory(server, &self.current_directory);
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
}