use crate::app::DirectoryItem;
use rupnp::ssdp::{SearchTarget, URN};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

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
    log::info!(target: "mop::upnp", "Starting UPnP discovery (rupnp + port scan in parallel)");
    let mut devices = Vec::new();

    // Run SSDP discovery and port scan in PARALLEL
    let ssdp_sender = sender.clone();

    let (ssdp_result, port_scan_result) = tokio::join!(
        ssdp_discovery(ssdp_sender),
        targeted_port_scan_parallel()
    );

    // Collect SSDP devices
    if let Ok(ssdp_devices) = ssdp_result {
        for device in ssdp_devices {
            if !devices
                .iter()
                .any(|d: &UpnpDevice| d.location == device.location)
            {
                devices.push(device);
            }
        }
    }

    sender.send(DiscoveryMessage::Phase1Complete).ok();
    sender.send(DiscoveryMessage::Phase2Complete).ok();

    // Collect port scan devices
    if let Ok(scan_devices) = port_scan_result {
        log::info!(target: "mop::upnp", "Port scan found {} devices", scan_devices.len());
        for device in scan_devices {
            if !devices
                .iter()
                .any(|d| is_same_discovered_device(d, &device))
            {
                sender
                    .send(DiscoveryMessage::DeviceFound(device.clone()))
                    .ok();
                devices.push(device);
            }
        }
    }

    log::info!(target: "mop::upnp", "Discovery complete: {} total devices", devices.len());
    sender.send(DiscoveryMessage::Phase3Complete).ok();
    sender.send(DiscoveryMessage::AllComplete(devices)).ok();
}

async fn ssdp_discovery(
    sender: Sender<DiscoveryMessage>,
) -> Result<Vec<UpnpDevice>, Box<dyn std::error::Error + Send + Sync>> {
    let mut devices = Vec::new();

    for search_target in ssdp_search_targets() {
        log::debug!(target: "mop::upnp", "SSDP discovery started, target={}, timeout=5s", search_target);

        match rupnp::discover(&search_target, Duration::from_secs(5), None).await {
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
                        log::info!(target: "mop::upnp", "SSDP found: {} ({})", friendly_name, device_url);

                        let base_url = if friendly_name.to_lowercase().contains("plex")
                            || device_type.contains("plex")
                        {
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

                        let content_directory_url =
                            match fetch_device_description(&device_url).await {
                                Ok(desc) => parse_content_directory_url(&desc, &device_url),
                                Err(_) => None,
                            };

                        let upnp_device = UpnpDevice {
                            name: format!("{} [{}]", friendly_name, device_type),
                            location: device_url,
                            base_url,
                            device_client: Some(device_type),
                            content_directory_url,
                        };

                        sender
                            .send(DiscoveryMessage::DeviceFound(upnp_device.clone()))
                            .ok();
                        if !devices
                            .iter()
                            .any(|d: &UpnpDevice| d.location == upnp_device.location)
                        {
                            devices.push(upnp_device);
                        }

                        if device_count >= 20 {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::error!(target: "mop::upnp", "SSDP discovery failed for {}: {}", search_target, e);
            }
        }
    }

    Ok(devices)
}

fn ssdp_search_targets() -> Vec<SearchTarget> {
    vec![
        SearchTarget::RootDevice,
        SearchTarget::URN(URN::device("schemas-upnp-org", "MediaServer", 1)),
    ]
}

async fn targeted_port_scan_parallel() -> Result<Vec<UpnpDevice>, Box<dyn std::error::Error + Send + Sync>> {
    log::debug!(target: "mop::upnp", "Starting parallel port scan");

    let network_base = match get_local_network() {
        Some(base) => {
            log::debug!(target: "mop::upnp", "Port scan using network {}.x", base);
            base
        }
        None => return Ok(Vec::new()),
    };

    let promising_ips = port_scan_host_suffixes();
    let media_ports = vec![32469, 32400, 8096, 8920];

    // Create all scan tasks
    log::info!(target: "mop::upnp", "Port scan: scanning {} IPs × {} ports = {} endpoints",
        promising_ips.len(), media_ports.len(), promising_ips.len() * media_ports.len());

    let mut tasks = Vec::new();
    for ip_suffix in &promising_ips {
        let ip = format!("{}.{}", network_base, ip_suffix);
        for &port in &media_ports {
            log::debug!(target: "mop::upnp", "Queuing scan: {}:{}", ip, port);
            let ip_clone = ip.clone();
            tasks.push(tokio::spawn(async move {
                let result = scan_single_endpoint(&ip_clone, port).await;
                if result.is_some() {
                    log::debug!(target: "mop::upnp", "Scan hit: {}:{}", ip_clone, port);
                }
                result
            }));
        }
    }

    // Run all scans in parallel and collect results
    log::debug!(target: "mop::upnp", "Port scan: waiting for {} parallel scans", tasks.len());
    let results = futures_util::future::join_all(tasks).await;
    log::debug!(target: "mop::upnp", "Port scan: all scans complete");

    let mut devices = Vec::new();
    for result in results {
        if let Ok(Some(device)) = result {
            if !devices
                .iter()
                .any(|d: &UpnpDevice| is_same_discovered_device(d, &device))
            {
                log::info!(target: "mop::upnp", "Port scan found: {}", device.name);
                devices.push(device);
            }
        }
    }

    log::info!(target: "mop::upnp", "Port scan complete: {} devices found", devices.len());
    Ok(devices)
}

async fn scan_single_endpoint(ip: &str, port: u16) -> Option<UpnpDevice> {
    let url = format!("http://{}:{}", ip, port);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()?;

    // For Plex DLNA port, try to get device description directly
    if port == 32469 {
        let desc_url = format!("{}/DeviceDescription.xml", url);
        if let Ok(response) = client.get(&desc_url).send().await {
            if response.status().is_success() {
                if let Ok(desc_text) = response.text().await {
                    // Parse device description for name and ContentDirectory URL
                    let friendly_name = extract_xml_value(&desc_text, "friendlyName")
                        .unwrap_or_else(|| format!("Plex DLNA ({})", ip));
                    let content_dir_url = parse_content_directory_url(&desc_text, &desc_url);
                    let base_url = dlna_device_base_url(ip, &url, &friendly_name, &desc_text);

                    log::info!(target: "mop::upnp", "Found Plex DLNA at {}: {}", url, friendly_name);
                    return Some(UpnpDevice {
                        name: format!("{} [MediaServer:1]", friendly_name),
                        location: desc_url,
                        base_url,
                        device_client: Some("Plex DLNA".to_string()),
                        content_directory_url: content_dir_url,
                    });
                }
            }
        }
        return None;
    }

    // For other ports, probe standard endpoints
    let endpoints = vec!["/", "/status", "/identity"];

    for endpoint in endpoints {
        let test_url = format!("{}{}", url, endpoint);
        if let Ok(response) = client.get(&test_url).send().await {
            let status = response.status();
            // Accept success OR 401 Unauthorized (Plex returns 401 when not authenticated)
            if status.is_success() || status.as_u16() == 401 {
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

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    if let Some(start) = xml.find(&open_tag) {
        let value_start = start + open_tag.len();
        if let Some(end) = xml[value_start..].find(&close_tag) {
            return Some(xml[value_start..value_start + end].to_string());
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
        format!(
            "{}://{}:{}",
            url.scheme(),
            url.host_str().unwrap_or(""),
            url.port().unwrap_or(80)
        )
    } else {
        return None;
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"service" => {
                    in_service = true;
                    current_service_type.clear();
                    current_control_url.clear();
                }
                b"serviceType" => in_service_type = true,
                b"controlURL" => in_control_url = true,
                _ => {}
            },
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
                        if current_service_type.contains("ContentDirectory")
                            && !current_control_url.is_empty()
                        {
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
            let port = url
                .port()
                .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
            format!("{}://{}:{}", url.scheme(), host, port)
        } else {
            device_url.to_string()
        }
    } else {
        device_url.to_string()
    }
}

fn get_local_network() -> Option<String> {
    // Get local IP from network interfaces directly
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for iface in interfaces {
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                let ip = v4.ip;
                // Skip loopback
                if ip.is_loopback() {
                    continue;
                }
                // Use first private IP found
                let octets = ip.octets();
                let is_private = matches!(octets[0], 10)
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168);

                if is_private {
                    let network = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
                    log::debug!(target: "mop::upnp", "Local network from {}: {}.x", iface.name, network);
                    return Some(network);
                }
            }
        }
    }
    log::warn!(target: "mop::upnp", "Could not determine local network");
    None
}

fn port_scan_host_suffixes() -> Vec<u8> {
    (1..=254).collect()
}

fn is_same_discovered_device(left: &UpnpDevice, right: &UpnpDevice) -> bool {
    left.location == right.location || left.base_url == right.base_url
}

fn dlna_device_base_url(
    ip: &str,
    dlna_url: &str,
    friendly_name: &str,
    device_description: &str,
) -> String {
    if friendly_name.to_lowercase().contains("plex")
        || device_description.to_lowercase().contains("plex")
    {
        format!("http://{}:32400", ip)
    } else {
        dlna_url.to_string()
    }
}

// Directory browsing implementation
pub fn browse_directory(
    server: &PlexServer,
    path: &[String],
    container_id_map: &mut std::collections::HashMap<Vec<String>, String>,
) -> (Vec<DirectoryItem>, Option<String>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async_browse_directory(server, path, container_id_map))
}

async fn async_browse_directory(
    server: &PlexServer,
    path: &[String],
    container_id_map: &mut std::collections::HashMap<Vec<String>, String>,
) -> (Vec<DirectoryItem>, Option<String>) {
    log::debug!(target: "mop::upnp", "Browsing directory: /{}", path.join("/"));
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
        log::debug!(target: "mop::soap", "SOAP Browse request to {} for container {}", content_dir_url, container_id);
        match browse_upnp_content_directory_with_id(content_dir_url, &container_id).await {
            Ok((upnp_items, container_mappings)) => {
                log::info!(target: "mop::upnp", "Browse returned {} items", upnp_items.len());
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
                log::error!(target: "mop::soap", "Browse failed for container {}: {}", container_id, e);
                errors.push(error_msg);
            }
        }
    } else {
        let error_msg = "No UPnP ContentDirectory service available".to_string();
        log::warn!(target: "mop::upnp", "{}", error_msg);
        errors.push(error_msg);
    }

    let error = errors
        .into_iter()
        .filter(|error| !error.trim().is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    (items, if error.is_empty() { None } else { Some(error) })
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

async fn browse_upnp_content_directory_with_id(
    content_dir_url: &str,
    container_id: &str,
) -> Result<(Vec<UpnpItem>, Vec<(String, String)>), Box<dyn std::error::Error>> {
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
        return Err(format!(
            "UPnP SOAP request failed with status: {}; body: {}",
            status, error_text
        )
        .into());
    }

    let response_text = response.text().await?;

    // Check for SOAP faults
    if response_text.contains("soap:Fault") || response_text.contains("SOAP-ENV:Fault") {
        return Err(format!("UPnP SOAP fault in response: {}", response_text).into());
    }

    parse_didl_response(&response_text)
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

fn parse_didl_response(
    xml: &str,
) -> Result<(Vec<UpnpItem>, Vec<(String, String)>), Box<dyn std::error::Error>> {
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
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
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
                        item.size = get_attribute_value(e, b"size").and_then(|s| s.parse().ok());
                        item.duration = get_attribute_value(e, b"duration");
                        item.format = get_attribute_value(e, b"protocolInfo")
                            .and_then(|p| p.split(':').nth(2).map(|s| s.to_string()));
                    }
                }
                _ => {}
            },
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
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                if in_title {
                    current_title = text;
                    if let Some(ref mut item) = current_item {
                        item.title = current_title.clone();
                    }
                } else if in_resource {
                    if let Some(ref mut item) = current_item {
                        item.resource_url = Some(text);
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

fn get_attribute_value(
    element: &quick_xml::events::BytesStart,
    attr_name: &[u8],
) -> Option<String> {
    element.attributes().find_map(|a| {
        if let Ok(attr) = a {
            if attr.key.as_ref() == attr_name {
                return Some(String::from_utf8_lossy(&attr.value).to_string());
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soap_response_with_result(result: &str) -> String {
        format!(
            r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
    <s:Body>
        <u:BrowseResponse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
            <Result>{}</Result>
        </u:BrowseResponse>
    </s:Body>
</s:Envelope>"#,
            result
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    }

    #[test]
    fn parses_non_ascii_title_from_cdata() {
        let didl = r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/">
    <container id="series-aeon">
        <dc:title><![CDATA[Æon Flux]]></dc:title>
    </container>
</DIDL-Lite>"#;

        let (items, mappings) = parse_didl_response(&soap_response_with_result(didl)).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Æon Flux");
        assert_eq!(
            mappings,
            vec![("Æon Flux".to_string(), "series-aeon".to_string())]
        );
    }

    #[test]
    fn port_scan_candidates_cover_full_private_subnet() {
        let candidates = port_scan_host_suffixes();

        assert!(candidates.contains(&31));
        assert!(candidates.contains(&1));
        assert!(candidates.contains(&254));
        assert!(!candidates.contains(&0));
        assert!(!candidates.contains(&255));
        assert_eq!(candidates.len(), 254);
    }

    #[test]
    fn ssdp_search_targets_include_media_servers() {
        let targets: Vec<String> = ssdp_search_targets()
            .into_iter()
            .map(|target| target.to_string())
            .collect();

        assert_eq!(
            targets,
            vec![
                "upnp:rootdevice".to_string(),
                "urn:schemas-upnp-org:device:MediaServer:1".to_string(),
            ]
        );
    }

    #[test]
    fn deduplicates_dlna_and_direct_plex_entries_by_base_url() {
        let dlna = UpnpDevice {
            name: "Plex Media Server: nasuntu".to_string(),
            location: "http://192.168.1.31:32469/DeviceDescription.xml".to_string(),
            base_url: "http://192.168.1.31:32400".to_string(),
            device_client: Some("urn:schemas-upnp-org:device:MediaServer:1".to_string()),
            content_directory_url: Some(
                "http://192.168.1.31:32469/ContentDirectory/control.xml".to_string(),
            ),
        };
        let direct = UpnpDevice {
            name: "Plex Server (192.168.1.31:32400)".to_string(),
            location: "http://192.168.1.31:32400".to_string(),
            base_url: "http://192.168.1.31:32400".to_string(),
            device_client: Some("DirectScan".to_string()),
            content_directory_url: None,
        };

        assert!(is_same_discovered_device(&dlna, &direct));
    }

    #[test]
    fn plex_dlna_scan_entries_use_plex_http_base_url() {
        let friendly_name = "Plex Media Server: nasuntu";
        let desc_text = "<manufacturer>Plex, Inc.</manufacturer>";
        let ip = "192.168.1.31";
        let dlna_url = format!("http://{}:32469", ip);
        let base_url = dlna_device_base_url(ip, &dlna_url, friendly_name, desc_text);

        assert_eq!(base_url, "http://192.168.1.31:32400");
    }
}
