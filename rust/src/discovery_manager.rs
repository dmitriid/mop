use crate::upnp_ssdp::{SsdpDiscovery, Device, DiscoveryError};
use crate::macos_permissions::{PermissionState, check_local_network_permission};
use crate::network_interfaces::{NetworkInterface, enumerate_network_interfaces, get_primary_interface};
use crate::app::DirectoryItem;
use std::time::Duration;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug)]
pub enum DiscoveryMessage {
    Started,
    PermissionCheckStarted,
    PermissionGranted,
    PermissionDenied(String),
    InterfaceFound(String),
    DeviceFound(crate::upnp::UpnpDevice),
    SsdpComplete(usize), // Number of devices found via SSDP
    PortScanStarted,
    PortScanComplete(usize), // Number of devices found via port scan
    AllComplete(Vec<crate::upnp::UpnpDevice>),
    Error(String),
}

pub struct DiscoveryManager {
    interfaces: Vec<NetworkInterface>,
    permission_state: PermissionState,
    devices: Vec<Device>,
}

impl DiscoveryManager {
    pub fn new() -> Self {
        Self {
            interfaces: Vec::new(),
            permission_state: PermissionState::Unknown,
            devices: Vec::new(),
        }
    }
    
    pub fn start_discovery() -> Receiver<DiscoveryMessage> {
        let (tx, rx) = mpsc::channel();
        
        std::thread::spawn(move || {
            let mut manager = DiscoveryManager::new();
            manager.run_discovery(tx);
        });
        
        rx
    }
    
    fn run_discovery(&mut self, sender: Sender<DiscoveryMessage>) {
        sender.send(DiscoveryMessage::Started).ok();
        
        // Step 1: Check permissions
        sender.send(DiscoveryMessage::PermissionCheckStarted).ok();
        
        self.permission_state = check_local_network_permission();
        match self.permission_state {
            PermissionState::Granted => {
                sender.send(DiscoveryMessage::PermissionGranted).ok();
            }
            PermissionState::Denied => {
                sender.send(DiscoveryMessage::PermissionDenied(
                    "Local network permission denied. UPnP discovery will not work.".to_string()
                )).ok();
                // Continue anyway to try port scanning
            }
            _ => {
                sender.send(DiscoveryMessage::Error(
                    "Could not determine permission state".to_string()
                )).ok();
            }
        }
        
        // Step 2: Enumerate network interfaces
        match enumerate_network_interfaces() {
            Ok(interfaces) => {
                self.interfaces = interfaces;
                for interface in &self.interfaces {
                    sender.send(DiscoveryMessage::InterfaceFound(
                        crate::network_interfaces::format_interface_info(interface)
                    )).ok();
                }
            }
            Err(e) => {
                sender.send(DiscoveryMessage::Error(format!("Network enumeration failed: {}", e))).ok();
            }
        }
        
        // Step 3: Attempt SSDP discovery if permissions allow
        let mut ssdp_devices = Vec::new();
        if self.permission_state == PermissionState::Granted {
            match self.discover_via_ssdp(&sender) {
                Ok(devices) => {
                    ssdp_devices = devices;
                }
                Err(e) => {
                    sender.send(DiscoveryMessage::Error(format!("SSDP discovery failed: {}", e))).ok();
                }
            }
        }
        
        sender.send(DiscoveryMessage::SsdpComplete(ssdp_devices.len())).ok();
        
        // Step 4: Port scanning fallback
        sender.send(DiscoveryMessage::PortScanStarted).ok();
        let port_scan_devices = match self.discover_via_port_scan(&sender) {
            Ok(devices) => devices,
            Err(e) => {
                sender.send(DiscoveryMessage::Error(format!("Port scan failed: {}", e))).ok();
                Vec::new()
            }
        };
        
        sender.send(DiscoveryMessage::PortScanComplete(port_scan_devices.len())).ok();
        
        // Step 5: Combine and deduplicate results
        let mut all_devices = ssdp_devices;
        for device in port_scan_devices {
            if !all_devices.iter().any(|d| d.location == device.location) {
                all_devices.push(device);
            }
        }
        
        // Convert to upnp::UpnpDevice format for compatibility
        let upnp_devices: Vec<crate::upnp::UpnpDevice> = all_devices.into_iter()
            .map(|d| self.convert_to_upnp_device(d))
            .collect();
        
        sender.send(DiscoveryMessage::AllComplete(upnp_devices)).ok();
    }
    
    fn discover_via_ssdp(&self, sender: &Sender<DiscoveryMessage>) -> Result<Vec<Device>, DiscoveryError> {
        let discovery = SsdpDiscovery::new()?;
        let devices = discovery.discover_devices()?;
        
        // Send each device as it's found
        for device in &devices {
            let upnp_device = self.convert_to_upnp_device(device.clone());
            sender.send(DiscoveryMessage::DeviceFound(upnp_device)).ok();
        }
        
        Ok(devices)
    }
    
    fn discover_via_port_scan(&self, _sender: &Sender<DiscoveryMessage>) -> Result<Vec<Device>, Box<dyn std::error::Error>> {
        // Get primary interface for port scanning
        let interface = match get_primary_interface() {
            Ok(iface) => iface,
            Err(_) => return Ok(Vec::new()), // No interface available
        };
        
        let network_range = match crate::network_interfaces::get_local_network_range(&interface) {
            Some(range) => range,
            None => return Ok(Vec::new()),
        };
        
        let mut devices = Vec::new();
        
        // Scan common media server ports on likely IPs
        let promising_ips = vec![1, 2, 10, 100, 200, 254];
        let media_ports = vec![32400, 8096, 8920]; // Plex, Jellyfin, Emby
        
        for ip_suffix in promising_ips {
            let ip = format!("{}.{}", network_range, ip_suffix);
            
            for &port in &media_ports {
                if let Some(device) = self.scan_endpoint(&ip, port) {
                    devices.push(device);
                }
            }
        }
        
        Ok(devices)
    }
    
    fn scan_endpoint(&self, ip: &str, port: u16) -> Option<Device> {
        let url = format!("http://{}:{}", ip, port);
        
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;
        
        // Try health endpoints
        let endpoints = vec!["/", "/status", "/identity"];
        
        for endpoint in endpoints {
            let test_url = format!("{}{}", url, endpoint);
            if let Ok(response) = client.get(&test_url).send() {
                if response.status().is_success() {
                    let server_name = match port {
                        32400 => format!("Plex Server ({}:{})", ip, port),
                        8096 => format!("Jellyfin Server ({}:{})", ip, port),
                        8920 => format!("Emby Server ({}:{})", ip, port),
                        _ => format!("Media Server ({}:{})", ip, port),
                    };
                    
                    return Some(Device {
                        name: server_name.clone(),
                        location: url.clone(),
                        base_url: url,
                        device_type: "MediaServer".to_string(),
                        manufacturer: "Port Scan".to_string(),
                        friendly_name: server_name,
                    });
                }
            }
        }
        
        None
    }
    
    fn convert_to_upnp_device(&self, device: Device) -> crate::upnp::UpnpDevice {
        let content_directory_url = self.find_content_directory_service(&device);
        
        crate::upnp::UpnpDevice {
            name: device.name,
            location: device.location,
            base_url: device.base_url,
            device_client: Some(device.manufacturer),
            content_directory_url,
        }
    }
    
    fn find_content_directory_service(&self, device: &Device) -> Option<String> {
        if let Ok(url) = url::Url::parse(&device.location) {
            if let Some(host) = url.host_str() {
                let port = url.port().unwrap_or(32400);
                return Some(format!("http://{}:{}/ContentDirectory/control", host, port));
            }
        }
        None
    }
}

// Enhanced discovery function for backward compatibility
pub fn discover_upnp_devices() -> (Vec<crate::upnp::UpnpDevice>, Vec<String>) {
    let receiver = DiscoveryManager::start_discovery();
    
    let mut devices = Vec::new();
    let mut errors = Vec::new();
    
    let timeout_duration = Duration::from_secs(15); // Increased timeout for permission dialogs
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout_duration {
        match receiver.try_recv() {
            Ok(message) => match message {
                DiscoveryMessage::DeviceFound(device) => {
                    devices.push(device);
                }
                DiscoveryMessage::AllComplete(final_devices) => {
                    return (final_devices, errors);
                }
                DiscoveryMessage::Error(e) => {
                    errors.push(e);
                }
                DiscoveryMessage::PermissionDenied(msg) => {
                    errors.push(msg);
                }
                _ => {} // Ignore other message types for now
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    
    if devices.is_empty() && errors.is_empty() {
        errors.push("Discovery timed out".to_string());
    }
    
    (devices, errors)
}

// Directory browsing remains the same for now
pub fn browse_directory(server: &crate::upnp::UpnpDevice, path: &[String]) -> (Vec<DirectoryItem>, Option<String>) {
    crate::upnp::browse_directory(server, path)
}