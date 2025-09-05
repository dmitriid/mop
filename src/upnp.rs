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
    
    // Search for all UPnP root devices using the new API
    match rupnp::discover(&SearchTarget::RootDevice, Duration::from_secs(5), None).await {
        Ok(device_stream) => {
            use futures_util::StreamExt;
            
            let mut stream = Box::pin(device_stream);
            let mut device_count = 0;
            
            while let Some(device_result) = stream.next().await {
                if let Ok(device) = device_result {
                    device_count += 1;
                    
                    let device_url = device.url().to_string();
                    let device_type = device.device_type().to_string();
                    let friendly_name = device.friendly_name().to_string();
                    
                    
                    // Special handling for Plex servers
                    let base_url = if friendly_name.to_lowercase().contains("plex") || 
                                      device_type.contains("plex") {
                        // For Plex, try to construct the correct URL with port 32400
                        if let Ok(url) = url::Url::parse(&device_url) {
                            if let Some(host) = url.host_str() {
                                format!("http://{}:32400", host)
                            } else {
                                extract_base_url(&device_url)
                            }
                        } else {
                            extract_base_url(&device_url)
                        }
                    } else {
                        extract_base_url(&device_url)
                    };
                    
                    // Fetch device description to get real service URLs
                    let content_directory_url = match fetch_device_description(&device_url).await {
                        Ok(desc) => parse_content_directory_url(&desc, &device_url),
                        Err(e) => {
                            None
                        }
                    };
                    
                    let upnp_device = UpnpDevice {
                        name: format!("{} [{}]", friendly_name, device_type),
                        location: device_url,
                        base_url,
                        device_client: Some(device_type),
                        content_directory_url,
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

async fn fetch_device_description(device_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .get(device_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to fetch device description: {}", response.status()).into());
    }
    
    Ok(response.text().await?)
}

fn parse_content_directory_url(device_desc: &str, device_url: &str) -> Option<String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    
    let mut reader = Reader::from_str(device_desc);
    reader.config_mut().trim_text(true);
    
    let mut buf = Vec::new();
    let mut in_service = false;
    let mut in_service_type = false;
    let mut in_control_url = false;
    let mut current_service_type = String::new();
    let mut current_control_url = String::new();
    
    // Parse the device URL to get base URL for relative paths
    let base_url = if let Ok(url) = url::Url::parse(device_url) {
        format!("{}://{}:{}", url.scheme(), url.host_str().unwrap_or(""), url.port().unwrap_or(80))
    } else {
        return None;
    };
    
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"service" => {
                        in_service = true;
                        current_service_type.clear();
                        current_control_url.clear();
                    }
                    b"serviceType" => in_service_type = true,
                    b"controlURL" => in_control_url = true,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_service {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if in_service_type {
                        current_service_type = text;
                    } else if in_control_url {
                        current_control_url = text;
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"service" => {
                        if current_service_type.contains("ContentDirectory") && !current_control_url.is_empty() {
                            // Resolve relative URL
                            let full_url = if current_control_url.starts_with("http") {
                                current_control_url
                            } else {
                                format!("{}{}", base_url, current_control_url)
                            };
                            return Some(full_url);
                        }
                        in_service = false;
                    }
                    b"serviceType" => in_service_type = false,
                    b"controlURL" => in_control_url = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("Error parsing device description: {}", e);
                break;
            }
            _ => {}
        }
        buf.clear();
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
pub fn browse_directory(server: &PlexServer, path: &[String], container_id_map: &mut std::collections::HashMap<Vec<String>, String>) -> (Vec<DirectoryItem>, Option<String>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async_browse_directory(server, path, container_id_map))
}

async fn async_browse_directory(server: &PlexServer, path: &[String], container_id_map: &mut std::collections::HashMap<Vec<String>, String>) -> (Vec<DirectoryItem>, Option<String>) {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    
    
    // Determine container ID based on path using proper nested traversal
    let container_id = if path.is_empty() {
        "0".to_string() // Root container
    } else {
        // Look up the container ID for the current path
        if let Some(id) = container_id_map.get(path) {
            id.clone()
        } else {
            // If not found, try to find it by traversing the path step by step
            let mut current_path = Vec::new();
            let mut current_id = "0".to_string();
            
            for segment in path {
                current_path.push(segment.clone());
                if let Some(id) = container_id_map.get(&current_path) {
                    current_id = id.clone();
                } else {
                    // If we can't find the path, we need to browse to discover it
                    // For now, fall back to root and let the discovery happen
                    current_id = "0".to_string();
                    break;
                }
            }
            current_id
        }
    };
    
    // Always use UPnP ContentDirectory service
    if let Some(content_dir_url) = &server.content_directory_url {
        match browse_upnp_content_directory_with_id(content_dir_url, &container_id).await {
            Ok((upnp_items, container_mappings)) => {
                // Update container ID mapping for navigation
                for (title, container_id) in &container_mappings {
                    // Store the mapping for this path + title combination
                    let mut new_path = path.to_vec();
                    new_path.push(title.clone());
                    container_id_map.insert(new_path, container_id.clone());
                }
                
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
                        let error_msg = format!("UPnP ContentDirectory failed: {}", e);
                        errors.push(error_msg);
                    }
        }
    } else {
        let error_msg = "No UPnP ContentDirectory service available".to_string();
        errors.push(error_msg);
    }

    // Try HTTP fallback only if UPnP fails
    match browse_http_directory(&server.base_url, path).await {
        Ok(http_items) => {
            items.extend(http_items);
            (items, if errors.is_empty() { None } else { Some(errors.join("; ")) })
        }
        Err(e) => {
            let error_msg = format!("HTTP browsing failed: {}", e);
            errors.push(error_msg);
            (items, Some(errors.join("; ")))
        }
    }
}


fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
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

async fn browse_upnp_content_directory_with_id(content_dir_url: &str, container_id: &str) -> Result<(Vec<UpnpItem>, Vec<(String, String)>), Box<dyn std::error::Error>> {
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    
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
        .header("User-Agent", "MOP/1.0")
        .body(soap_body)
        .send()
        .await?;
    
    let status = response.status();
    
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("UPnP SOAP request failed with status: {}", status).into());
    }
    
    let response_text = response.text().await?;
    
    // Check for SOAP faults
    if response_text.contains("soap:Fault") || response_text.contains("SOAP-ENV:Fault") {
        return Err(format!("UPnP SOAP fault in response: {}", response_text).into());
    }
    
    parse_didl_response(&response_text)
}

async fn browse_upnp_content_directory(content_dir_url: &str, path: &[String]) -> Result<Vec<UpnpItem>, Box<dyn std::error::Error>> {
    // For now, always use root container ID
    // This is a temporary fix - proper implementation would track container IDs
    let container_id = "0";
    
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    
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
        .header("User-Agent", "MOP/1.0")
        .body(soap_body)
        .send()
        .await?;
    
    let status = response.status();
    
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("UPnP SOAP request failed with status: {}", status).into());
    }
    
    let response_text = response.text().await?;
    
    // Check for SOAP faults
    if response_text.contains("soap:Fault") || response_text.contains("SOAP-ENV:Fault") {
        return Err(format!("UPnP SOAP fault in response: {}", response_text).into());
    }
    
    let (items, _) = parse_didl_response(&response_text)?;
    Ok(items)
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

fn extract_didl_from_soap(soap_xml: &str) -> Result<String, Box<dyn std::error::Error>> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    
    let mut reader = Reader::from_str(soap_xml);
    reader.config_mut().trim_text(true);
    
    let mut buf = Vec::new();
    let mut in_result = false;
    
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"Result" {
                    in_result = true;
                }
            }
            Ok(Event::Text(e)) => {
                if in_result {
                    // Unescape the XML entities
                    let escaped = e.unescape().unwrap_or_default();
                    return Ok(escaped.to_string());
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"Result" {
                    in_result = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Box::new(e)),
            _ => {}
        }
        buf.clear();
    }
    
    Err("No Result element found in SOAP response".into())
}

fn parse_didl_response(xml: &str) -> Result<(Vec<UpnpItem>, Vec<(String, String)>), Box<dyn std::error::Error>> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    
    // First, extract the DIDL-Lite XML from the SOAP response
    let didl_xml = extract_didl_from_soap(xml)?;
    
    let mut items = Vec::new();
    let mut container_mappings = Vec::new(); // (title, container_id)
    let mut reader = Reader::from_str(&didl_xml);
    reader.config_mut().trim_text(true);
    
    let mut buf = Vec::new();
    let mut current_item: Option<UpnpItem> = None;
    let mut in_title = false;
    let mut in_resource = false;
    let mut current_title = String::new();
    
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"container" => {
                        let id = get_attribute_value(e, b"id").unwrap_or_default();
                        current_item = Some(UpnpItem {
                            id: id.clone(),
                            title: String::new(),
                            is_container: true,
                            resource_url: None,
                            size: None,
                            duration: None,
                            format: None,
                        });
                        current_title.clear();
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
                    current_title = e.unescape().unwrap_or_default().to_string();
                    if let Some(ref mut item) = current_item {
                        item.title = current_title.clone();
                    }
                } else if in_resource {
                    if let Some(ref mut item) = current_item {
                        item.resource_url = Some(e.unescape().unwrap_or_default().to_string());
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"container" => {
                        if let Some(item) = current_item.take() {
                            if !current_title.is_empty() {
                                // Store container mapping for navigation
                                container_mappings.push((current_title.clone(), item.id.clone()));
                            }
                            items.push(item);
                        }
                    }
                    b"item" => {
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
    
    Ok((items, container_mappings))
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
