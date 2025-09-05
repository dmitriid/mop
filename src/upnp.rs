use crate::app::DirectoryItem;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use rupnp::ssdp::SearchTarget;

#[derive(Debug, Clone)]
pub struct UpnpDevice {
    pub name: String,
    pub location: String,
    pub base_url: String,
    pub device_client: Option<String>,
    pub content_directory_url: Option<String>,
}

pub type PlexServer = UpnpDevice;

#[derive(Debug)]
pub enum DiscoveryMessage {
    Started,
    DeviceFound(UpnpDevice),
    Phase1Complete, // SSDP discovery complete
    Phase2Complete, // Extended discovery complete
    Phase3Complete, // Port scan complete
    AllComplete(Vec<UpnpDevice>),
    Error(String),
}

pub fn start_discovery() -> Receiver<DiscoveryMessage> {
    let (tx, rx) = mpsc::channel();
    
    std::thread::spawn(move || {
        tx.send(DiscoveryMessage::Started).ok();
        
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(discover_with_rupnp(tx));
    });
    
    rx
}

async fn discover_with_rupnp(sender: Sender<DiscoveryMessage>) {
    let mut devices = Vec::new();
    
    // Send helpful startup info
    sender.send(DiscoveryMessage::Error("Starting UPnP discovery with rupnp library...".to_string())).ok();
    
    // Search for all UPnP root devices using the new API
    match rupnp::discover(&SearchTarget::RootDevice, Duration::from_secs(5), None).await {
        Ok(device_stream) => {
            use futures_util::StreamExt;
            
            let mut stream = Box::pin(device_stream);
            let mut device_count = 0;
            
            while let Some(device_result) = stream.next().await {
                if let Ok(device) = device_result {
                    device_count += 1;
                    
                    let upnp_device = UpnpDevice {
                        name: format!("{} [{}]", device.friendly_name(), device.device_type()),
                        location: device.url().to_string(),
                        base_url: extract_base_url(&device.url().to_string()),
                        device_client: Some(device.device_type().to_string()),
                        content_directory_url: find_content_directory_service(&device),
                    };
                    
                    sender.send(DiscoveryMessage::DeviceFound(upnp_device.clone())).ok();
                    devices.push(upnp_device);
                    
                    if device_count >= 20 {
                        break; // Limit to prevent hanging
                    }
                }
            }
        }
        Err(e) => {
            sender.send(DiscoveryMessage::Error(format!("UPnP discovery failed: {}", e))).ok();
        }
    }
    
    sender.send(DiscoveryMessage::Phase1Complete).ok();
    
    // Note: rupnp 3.0 discovery already finds all devices including media servers
    
    sender.send(DiscoveryMessage::Phase2Complete).ok();
    
    // Try port scanning as fallback
    match targeted_port_scan().await {
        Ok(scan_devices) => {
            for device in scan_devices {
                if !devices.iter().any(|d| d.location == device.location) {
                    sender.send(DiscoveryMessage::DeviceFound(device.clone())).ok();
                    devices.push(device);
                }
            }
        }
        Err(e) => {
            sender.send(DiscoveryMessage::Error(format!("Port scan failed: {}", e))).ok();
        }
    }
    
    sender.send(DiscoveryMessage::Phase3Complete).ok();
    sender.send(DiscoveryMessage::AllComplete(devices)).ok();
}

async fn targeted_port_scan() -> Result<Vec<UpnpDevice>, Box<dyn std::error::Error>> {
    let mut devices = Vec::new();
    
    // Get local network range
    let network_base = match get_local_network() {
        Some(base) => base,
        None => return Ok(devices), // Return empty instead of error
    };
    
    // Scan promising IPs and ports
    let promising_ips = vec![1, 2, 10, 100, 200];
    let media_ports = vec![32400, 8096, 8920]; // Plex, Jellyfin, Emby
    
    for ip_suffix in promising_ips {
        let ip = format!("{}.{}", network_base, ip_suffix);
        for &port in &media_ports {
            if let Some(device) = scan_single_endpoint(&ip, port).await {
                devices.push(device);
            }
        }
    }
    
    Ok(devices)
}

async fn scan_single_endpoint(ip: &str, port: u16) -> Option<UpnpDevice> {
    let url = format!("http://{}:{}", ip, port);
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .ok()?;
    
    let endpoints = vec!["/", "/status", "/identity"];
    
    for endpoint in endpoints {
        let test_url = format!("{}{}", url, endpoint);
        if let Ok(response) = client.get(&test_url).send().await {
            if response.status().is_success() {
                let server_name = match port {
                    32400 => format!("Plex Server ({}:{})", ip, port),
                    8096 => format!("Jellyfin Server ({}:{})", ip, port),
                    8920 => format!("Emby Server ({}:{})", ip, port),
                    _ => format!("Media Server ({}:{})", ip, port),
                };
                
                return Some(UpnpDevice {
                    name: server_name,
                    location: url.clone(),
                    base_url: url,
                    device_client: Some("DirectScan".to_string()),
                    content_directory_url: None,
                });
            }
        }
    }
    
    None
}

fn find_content_directory_service(device: &rupnp::Device) -> Option<String> {
    // Look for ContentDirectory service
    for service in device.services() {
        if service.service_type().to_string().contains("ContentDirectory") {
            // In rupnp 3.0, we need to construct the control URL manually
            let device_url = device.url();
            if let Ok(url) = url::Url::parse(&device_url.to_string()) {
                if let Some(host) = url.host_str() {
                    let port = url.port().unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
                    return Some(format!("{}://{}:{}/ContentDirectory/control", url.scheme(), host, port));
                }
            }
        }
    }
    
    // Fallback: construct URL based on device location
    if let Ok(url) = url::Url::parse(&device.url().to_string()) {
        if let Some(host) = url.host_str() {
            let port = url.port().unwrap_or(32400);
            return Some(format!("http://{}:{}/ContentDirectory/control", host, port));
        }
    }
    
    None
}

fn extract_base_url(device_url: &str) -> String {
    if let Ok(url) = url::Url::parse(device_url) {
        if let Some(host) = url.host_str() {
            let port = url.port().unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
            format!("{}://{}:{}", url.scheme(), host, port)
        } else {
            device_url.to_string()
        }
    } else {
        device_url.to_string()
    }
}

fn get_local_network() -> Option<String> {
    use std::net::{TcpStream, SocketAddr};
    
    if let Ok(stream) = TcpStream::connect("8.8.8.8:80") {
        if let Ok(local_addr) = stream.local_addr() {
            if let SocketAddr::V4(addr) = local_addr {
                let ip = addr.ip().to_string();
                let parts: Vec<&str> = ip.split('.').collect();
                if parts.len() >= 3 {
                    return Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]));
                }
            }
        }
    }
    None
}

// Public API functions - simplified blocking version
pub fn discover_plex_servers() -> (Vec<PlexServer>, Vec<String>) {
    let receiver = start_discovery();
    
    let mut devices = Vec::new();
    let mut errors = Vec::new();
    
    let timeout_duration = Duration::from_secs(10);
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
                _ => {} // Ignore intermediate phase completions
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    
    if devices.is_empty() && errors.is_empty() {
        errors.push("Discovery timed out".to_string());
    }
    
    (devices, errors)
}

// Directory browsing implementation
pub fn browse_directory(server: &PlexServer, path: &[String]) -> (Vec<DirectoryItem>, Option<String>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async_browse_directory(server, path))
}

async fn async_browse_directory(server: &PlexServer, path: &[String]) -> (Vec<DirectoryItem>, Option<String>) {
    let mut items = Vec::new();
    
    // For UPnP devices, try to browse via ContentDirectory service
    if let Some(content_dir_url) = &server.content_directory_url {
        match browse_upnp_content_directory(content_dir_url, path).await {
            Ok(upnp_items) => {
                for item in upnp_items {
                    items.push(DirectoryItem {
                        name: item.title,
                        is_directory: item.is_container,
                        url: item.resource_url,
                        metadata: if item.is_container {
                            None
                        } else {
                            Some(crate::app::FileMetadata {
                                size: item.size,
                                duration: item.duration,
                                format: item.format,
                            })
                        },
                    });
                }
                return (items, None);
            }
            Err(e) => {
                return (items, Some(format!("UPnP browsing failed: {}", e)));
            }
        }
    }
    
    // Fallback: try direct HTTP browsing for media servers
    match browse_http_directory(&server.base_url, path).await {
        Ok(http_items) => {
            items.extend(http_items);
            (items, None)
        }
        Err(e) => {
            (items, Some(format!("HTTP browsing failed: {}", e)))
        }
    }
}

#[derive(Debug, Clone)]
struct UpnpItem {
    id: String,
    title: String,
    is_container: bool,
    resource_url: Option<String>,
    size: Option<u64>,
    duration: Option<String>,
    format: Option<String>,
}

async fn browse_upnp_content_directory(content_dir_url: &str, path: &[String]) -> Result<Vec<UpnpItem>, Box<dyn std::error::Error>> {
    let container_id = path_to_container_id(path);
    
    let client = reqwest::Client::new();
    
    // SOAP request for UPnP ContentDirectory Browse action
    let soap_action = "urn:schemas-upnp-org:service:ContentDirectory:1#Browse";
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
        <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
            <s:Body>
                <u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
                    <ObjectID>{}</ObjectID>
                    <BrowseFlag>BrowseDirectChildren</BrowseFlag>
                    <Filter>*</Filter>
                    <StartingIndex>0</StartingIndex>
                    <RequestedCount>100</RequestedCount>
                    <SortCriteria></SortCriteria>
                </u:Browse>
            </s:Body>
        </s:Envelope>"#,
        container_id
    );
    
    let response = client
        .post(content_dir_url)
        .header("Content-Type", "text/xml; charset=utf-8")
        .header("SOAPAction", format!("\"{}\"", soap_action))
        .body(soap_body)
        .send()
        .await?;
    
    let response_text = response.text().await?;
    parse_didl_response(&response_text)
}

async fn browse_http_directory(base_url: &str, path: &[String]) -> Result<Vec<DirectoryItem>, Box<dyn std::error::Error>> {
    let mut items = Vec::new();
    let client = reqwest::Client::new();
    
    // Try common media server endpoints
    let endpoints: Vec<String> = if path.is_empty() {
        vec![
            "/library/sections".to_string(),      // Plex
            "/web/index.html".to_string(),        // Plex web
            "/Users".to_string(),                 // Jellyfin/Emby users
            "/Items".to_string(),                 // Jellyfin/Emby items
            "/".to_string(),                      // Root directory
        ]
    } else {
        vec![format!("/{}", path.join("/"))]
    };
    
    for endpoint in endpoints {
        let url = format!("{}{}", base_url, endpoint);
        
        if let Ok(response) = client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(text) = response.text().await {
                    // Try to parse as JSON (modern media servers)
                    if let Ok(json_items) = parse_json_directory(&text) {
                        items.extend(json_items);
                        break;
                    }
                    
                    // Try to parse as HTML directory listing
                    if let Ok(html_items) = parse_html_directory(&text, &url) {
                        items.extend(html_items);
                        break;
                    }
                }
            }
        }
    }
    
    if items.is_empty() {
        return Err("No browsable content found".into());
    }
    
    Ok(items)
}

fn parse_json_directory(json_text: &str) -> Result<Vec<DirectoryItem>, Box<dyn std::error::Error>> {
    // Try to parse common JSON structures from media servers
    let mut items = Vec::new();
    
    // Simple JSON parsing for basic structures
    if json_text.contains("\"MediaContainer\"") {
        // Plex-style response
        items.push(DirectoryItem {
            name: "Plex Media Server".to_string(),
            is_directory: true,
            url: None,
            metadata: None,
        });
    } else if json_text.contains("\"Items\"") {
        // Jellyfin/Emby-style response  
        items.push(DirectoryItem {
            name: "Media Library".to_string(),
            is_directory: true,
            url: None,
            metadata: None,
        });
    }
    
    Ok(items)
}

fn parse_html_directory(html_text: &str, base_url: &str) -> Result<Vec<DirectoryItem>, Box<dyn std::error::Error>> {
    let mut items = Vec::new();
    
    // Simple HTML parsing for directory listings
    for line in html_text.lines() {
        if line.contains("<a href=") && !line.contains("Parent Directory") {
            if let Some(start) = line.find("href=\"") {
                if let Some(end) = line[start + 6..].find("\"") {
                    let href = &line[start + 6..start + 6 + end];
                    
                    // Extract filename from href
                    let name = href.trim_start_matches('/').trim_end_matches('/');
                    if !name.is_empty() && name != ".." {
                        let is_directory = href.ends_with('/');
                        let full_url = if href.starts_with("http") {
                            href.to_string()
                        } else {
                            format!("{}/{}", base_url.trim_end_matches('/'), href.trim_start_matches('/'))
                        };
                        
                        items.push(DirectoryItem {
                            name: name.to_string(),
                            is_directory,
                            url: if is_directory { None } else { Some(full_url) },
                            metadata: None,
                        });
                    }
                }
            }
        }
    }
    
    Ok(items)
}

fn parse_didl_response(xml: &str) -> Result<Vec<UpnpItem>, Box<dyn std::error::Error>> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    
    let mut items = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    
    let mut buf = Vec::new();
    let mut current_item: Option<UpnpItem> = None;
    let mut in_title = false;
    let mut in_resource = false;
    
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"container" => {
                        let id = get_attribute_value(e, b"id").unwrap_or_default();
                        current_item = Some(UpnpItem {
                            id,
                            title: String::new(),
                            is_container: true,
                            resource_url: None,
                            size: None,
                            duration: None,
                            format: None,
                        });
                    }
                    b"item" => {
                        let id = get_attribute_value(e, b"id").unwrap_or_default();
                        current_item = Some(UpnpItem {
                            id,
                            title: String::new(),
                            is_container: false,
                            resource_url: None,
                            size: None,
                            duration: None,
                            format: None,
                        });
                    }
                    b"dc:title" => in_title = true,
                    b"res" => {
                        in_resource = true;
                        if let Some(ref mut item) = current_item {
                            item.size = get_attribute_value(e, b"size")
                                .and_then(|s| s.parse().ok());
                            item.duration = get_attribute_value(e, b"duration");
                            item.format = get_attribute_value(e, b"protocolInfo")
                                .and_then(|p| p.split(':').nth(2).map(|s| s.to_string()));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_title {
                    if let Some(ref mut item) = current_item {
                        item.title = e.unescape().unwrap_or_default().to_string();
                    }
                } else if in_resource {
                    if let Some(ref mut item) = current_item {
                        item.resource_url = Some(e.unescape().unwrap_or_default().to_string());
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"container" | b"item" => {
                        if let Some(item) = current_item.take() {
                            items.push(item);
                        }
                    }
                    b"dc:title" => in_title = false,
                    b"res" => in_resource = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Box::new(e)),
            _ => {}
        }
        buf.clear();
    }
    
    Ok(items)
}

fn get_attribute_value(element: &quick_xml::events::BytesStart, attr_name: &[u8]) -> Option<String> {
    element
        .attributes()
        .find_map(|a| {
            if let Ok(attr) = a {
                if attr.key.as_ref() == attr_name {
                    return Some(String::from_utf8_lossy(&attr.value).to_string());
                }
            }
            None
        })
}

fn path_to_container_id(path: &[String]) -> String {
    if path.is_empty() {
        "0".to_string() // Root container in UPnP
    } else {
        // Simple path to ID mapping - in real implementation you'd need to track container IDs
        format!("path_{}", path.join("_"))
    }
}