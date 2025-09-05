use crate::app::DirectoryItem;
use std::sync::mpsc::Receiver;

#[derive(Debug, Clone)]
pub struct UpnpDevice {
    pub name: String,
    pub location: String,
    pub base_url: String,
    pub device_client: Option<String>,
    pub content_directory_url: Option<String>,
}

// Keep PlexServer as an alias for backward compatibility
pub type PlexServer = UpnpDevice;

#[derive(Debug)]
pub enum DiscoveryMessage {
    Started,
    DeviceFound(UpnpDevice),
    Phase1Complete, // SSDP discovery complete
    Phase2Complete, // Extended discovery complete
    Phase3Complete, // Port scan complete
    AllComplete(Vec<UpnpDevice>),    // Final result
    Error(String),
}

pub fn start_discovery() -> Receiver<DiscoveryMessage> {
    let (tx, rx) = std::sync::mpsc::channel();
    
    std::thread::spawn(move || {
        let discovery_rx = crate::discovery_manager::DiscoveryManager::start_discovery();
        
        // Convert discovery manager messages to upnp messages
        while let Ok(msg) = discovery_rx.recv() {
            let upnp_msg = match msg {
                crate::discovery_manager::DiscoveryMessage::Started => DiscoveryMessage::Started,
                crate::discovery_manager::DiscoveryMessage::DeviceFound(device) => DiscoveryMessage::DeviceFound(device),
                crate::discovery_manager::DiscoveryMessage::AllComplete(devices) => DiscoveryMessage::AllComplete(devices),
                crate::discovery_manager::DiscoveryMessage::Error(e) => DiscoveryMessage::Error(e),
                crate::discovery_manager::DiscoveryMessage::PermissionDenied(e) => DiscoveryMessage::Error(e),
                crate::discovery_manager::DiscoveryMessage::SsdpComplete(_) => DiscoveryMessage::Phase1Complete,
                crate::discovery_manager::DiscoveryMessage::PortScanComplete(_) => DiscoveryMessage::Phase2Complete,
                _ => continue, // Skip other message types
            };
            
            if tx.send(upnp_msg).is_err() {
                break; // Receiver disconnected
            }
        }
    });
    
    rx
}

// Public API functions - simplified blocking version
pub fn discover_plex_servers() -> (Vec<PlexServer>, Vec<String>) {
    // Use the new discovery manager
    crate::discovery_manager::discover_upnp_devices()
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