use std::net::{UdpSocket, SocketAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use std::io::{self, ErrorKind};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub location: String,
    pub base_url: String,
    pub device_type: String,
    pub manufacturer: String,
    pub friendly_name: String,
}

#[derive(Debug)]
pub enum DiscoveryError {
    NetworkError(io::Error),
    PermissionDenied,
    NoDevicesFound,
    ParseError(String),
    Timeout,
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::NetworkError(e) => write!(f, "Network error: {}", e),
            DiscoveryError::PermissionDenied => write!(f, "Local network permission denied"),
            DiscoveryError::NoDevicesFound => write!(f, "No UPnP devices found on network"),
            DiscoveryError::ParseError(e) => write!(f, "Failed to parse device response: {}", e),
            DiscoveryError::Timeout => write!(f, "Discovery timeout"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl From<io::Error> for DiscoveryError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            ErrorKind::PermissionDenied => DiscoveryError::PermissionDenied,
            _ => DiscoveryError::NetworkError(e),
        }
    }
}

pub struct SsdpDiscovery {
    socket: UdpSocket,
    multicast_addr: SocketAddr,
    timeout: Duration,
}

impl SsdpDiscovery {
    pub fn new() -> Result<Self, DiscoveryError> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| {
                if e.kind() == ErrorKind::PermissionDenied {
                    DiscoveryError::PermissionDenied
                } else {
                    DiscoveryError::NetworkError(e)
                }
            })?;
        log::info!(target: "mop::net", "SSDP socket bound to 0.0.0.0:0");

        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_write_timeout(Some(Duration::from_millis(1000)))?;
        socket.set_nonblocking(false)?;
        log::debug!(target: "mop::net", "Socket read timeout: 100ms, write timeout: 1000ms");

        let multicast_addr: SocketAddr = "239.255.255.250:1900".parse()
            .map_err(|e| DiscoveryError::ParseError(format!("Invalid multicast address: {}", e)))?;

        // Join multicast group with detailed error handling
        let multicast_ip = Ipv4Addr::new(239, 255, 255, 250);
        let interface_ip = Ipv4Addr::new(0, 0, 0, 0);

        socket.join_multicast_v4(&multicast_ip, &interface_ip)
            .map_err(|e| {
                match e.kind() {
                    ErrorKind::PermissionDenied => DiscoveryError::PermissionDenied,
                    _ => DiscoveryError::NetworkError(e),
                }
            })?;
        log::info!(target: "mop::net", "Joined multicast group 239.255.255.250 on interface 0.0.0.0");

        Ok(Self {
            socket,
            multicast_addr,
            timeout: Duration::from_secs(5),
        })
    }
    
    pub fn discover_devices(&self) -> Result<Vec<Device>, DiscoveryError> {
        // Send M-SEARCH request
        let search_request = "M-SEARCH * HTTP/1.1\r\n\
                             HOST: 239.255.255.250:1900\r\n\
                             MAN: \"ssdp:discover\"\r\n\
                             ST: upnp:rootdevice\r\n\
                             MX: 3\r\n\r\n";
        
        self.socket.send_to(search_request.as_bytes(), self.multicast_addr)
            .map_err(|e| {
                match e.kind() {
                    ErrorKind::PermissionDenied => DiscoveryError::PermissionDenied,
                    _ => DiscoveryError::NetworkError(e),
                }
            })?;
        log::info!(target: "mop::ssdp", "Sent M-SEARCH for upnp:rootdevice to 239.255.255.250:1900");

        // Also send search for media devices specifically
        let media_search = "M-SEARCH * HTTP/1.1\r\n\
                           HOST: 239.255.255.250:1900\r\n\
                           MAN: \"ssdp:discover\"\r\n\
                           ST: urn:schemas-upnp-org:device:MediaServer:1\r\n\
                           MX: 3\r\n\r\n";
        
        let _ = self.socket.send_to(media_search.as_bytes(), self.multicast_addr);
        log::info!(target: "mop::ssdp", "Sent M-SEARCH for MediaServer:1 to 239.255.255.250:1900");

        // Collect responses with deduplication
        let mut devices = HashMap::new();
        let start_time = Instant::now();
        
        while start_time.elapsed() < self.timeout {
            let mut buf = [0; 4096];
            match self.socket.recv_from(&mut buf) {
                Ok((size, addr)) => {
                    if let Ok(response) = std::str::from_utf8(&buf[..size]) {
                        if let Some(device) = self.parse_ssdp_response(response, addr) {
                            log::debug!(target: "mop::ssdp", "SSDP response from {}: {}", addr, device.location);
                            // Use location as key to avoid duplicates
                            devices.insert(device.location.clone(), device);
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    continue;
                }
                Err(e) => {
                    if devices.is_empty() {
                        return Err(DiscoveryError::NetworkError(e));
                    } else {
                        // We got some devices, ignore further errors
                        break;
                    }
                }
            }
        }
        
        let device_list: Vec<Device> = devices.into_values().collect();
        log::info!(target: "mop::ssdp", "SSDP discovery complete: found {} devices", device_list.len());

        if device_list.is_empty() {
            Err(DiscoveryError::NoDevicesFound)
        } else {
            Ok(device_list)
        }
    }
    
    fn parse_ssdp_response(&self, response: &str, _addr: SocketAddr) -> Option<Device> {
        // Only process HTTP 200 OK responses
        if !response.starts_with("HTTP/1.1 200 OK") {
            return None;
        }
        
        let mut location = None;
        let mut server = None;
        let mut st = None;
        let mut usn = None;
        
        for line in response.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            if let Some(colon_pos) = line.find(':') {
                let (header, value) = line.split_at(colon_pos);
                let header = header.trim().to_lowercase();
                let value = value[1..].trim(); // Skip the ':'
                
                match header.as_str() {
                    "location" => location = Some(value.to_string()),
                    "server" => server = Some(value.to_string()),
                    "st" => st = Some(value.to_string()),
                    "usn" => usn = Some(value.to_string()),
                    _ => {}
                }
            }
        }
        
        let location = location?;
        let base_url = self.extract_base_url(&location);
        let device_type = st.unwrap_or_else(|| "Unknown".to_string());
        let manufacturer = server.unwrap_or_else(|| "Unknown".to_string());
        
        // Extract friendly name from USN or use device type
        let friendly_name = if let Some(usn) = &usn {
            if let Some(uuid_start) = usn.find("uuid:") {
                let uuid_part = &usn[uuid_start + 5..];
                if let Some(uuid_end) = uuid_part.find("::") {
                    format!("Device-{}", &uuid_part[..uuid_end.min(8)])
                } else {
                    format!("Device-{}", &uuid_part[..uuid_part.len().min(8)])
                }
            } else {
                device_type.clone()
            }
        } else {
            device_type.clone()
        };
        
        let display_name = if manufacturer != "Unknown" {
            format!("{} [{}] ({})", friendly_name, device_type, manufacturer)
        } else {
            format!("{} [{}]", friendly_name, device_type)
        };
        
        Some(Device {
            name: display_name,
            location: location.clone(),
            base_url,
            device_type,
            manufacturer,
            friendly_name,
        })
    }
    
    fn extract_base_url(&self, location: &str) -> String {
        if let Ok(url) = url::Url::parse(location) {
            if let Some(host) = url.host_str() {
                let port = url.port().unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
                return format!("{}://{}:{}", url.scheme(), host, port);
            }
        }
        location.to_string()
    }
}

// Test if multicast capability is available
pub fn test_multicast_capability() -> Result<(), DiscoveryError> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    log::debug!(target: "mop::net", "Multicast test: socket bound");
    socket.set_write_timeout(Some(Duration::from_millis(500)))?;

    let multicast_ip = Ipv4Addr::new(239, 255, 255, 250);
    let interface_ip = Ipv4Addr::new(0, 0, 0, 0);

    socket.join_multicast_v4(&multicast_ip, &interface_ip)?;
    log::debug!(target: "mop::net", "Multicast test: joined group 239.255.255.250");

    let test_message = b"TEST";
    let multicast_addr: SocketAddr = "239.255.255.250:1900".parse()
        .map_err(|e| DiscoveryError::ParseError(format!("Invalid address: {}", e)))?;

    socket.send_to(test_message, multicast_addr)?;
    log::debug!(target: "mop::net", "Multicast test: sent test packet");

    Ok(())
}