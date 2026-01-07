use std::net::Ipv4Addr;
use std::collections::HashMap;
use if_addrs::{get_if_addrs, IfAddr};

#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub is_loopback: bool,
    pub supports_multicast: bool,
    pub has_upnp_devices: Option<bool>, // None = not tested yet
}

#[derive(Debug)]
pub enum NetworkError {
    EnumerationFailed(String),
    TestFailed(String),
    NoValidInterfaces,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::EnumerationFailed(e) => write!(f, "Failed to enumerate network interfaces: {}", e),
            NetworkError::TestFailed(e) => write!(f, "Network interface test failed: {}", e),
            NetworkError::NoValidInterfaces => write!(f, "No valid network interfaces found"),
        }
    }
}

impl std::error::Error for NetworkError {}

pub fn enumerate_network_interfaces() -> Result<Vec<NetworkInterface>, NetworkError> {
    log::debug!(target: "mop::net", "Enumerating network interfaces");
    let interfaces = get_if_addrs()
        .map_err(|e| NetworkError::EnumerationFailed(format!("System error: {}", e)))?;
    log::debug!(target: "mop::net", "Found {} raw interfaces", interfaces.len());
    
    let mut result = Vec::new();
    let mut seen_ips = HashMap::new();
    
    for interface in interfaces {
        if let IfAddr::V4(v4_addr) = interface.addr {
            let ip = v4_addr.ip;
            
            // Skip if we've already seen this IP (duplicate interfaces)
            if seen_ips.contains_key(&ip) {
                continue;
            }
            seen_ips.insert(ip, true);
            
            // Skip localhost unless it's the only interface
            if ip.is_loopback() {
                continue;
            }
            
            // Basic multicast capability check
            let supports_multicast = !ip.is_loopback() && 
                                    !ip.is_broadcast() && 
                                    !ip.is_multicast() &&
                                    v4_addr.broadcast.is_some();
            
            log::info!(target: "mop::net", "Found interface {} ({}) multicast={}",
                interface.name, ip, supports_multicast);
            result.push(NetworkInterface {
                name: interface.name,
                ip,
                is_loopback: ip.is_loopback(),
                supports_multicast,
                has_upnp_devices: None,
            });
        }
    }

    // If no non-loopback interfaces found, include loopback
    if result.is_empty() {
        for interface in get_if_addrs().map_err(|e| NetworkError::EnumerationFailed(format!("System error: {}", e)))? {
            if let IfAddr::V4(v4_addr) = interface.addr {
                let ip = v4_addr.ip;
                if ip.is_loopback() {
                    result.push(NetworkInterface {
                        name: interface.name,
                        ip,
                        is_loopback: true,
                        supports_multicast: false,
                        has_upnp_devices: None,
                    });
                    break; // Only add one loopback interface
                }
            }
        }
    }
    
    if result.is_empty() {
        return Err(NetworkError::NoValidInterfaces);
    }
    
    // Sort by preference: non-loopback first, then by IP
    result.sort_by(|a, b| {
        match (a.is_loopback, b.is_loopback) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.ip.cmp(&b.ip),
        }
    });

    log::info!(target: "mop::net", "Enumerated {} valid network interfaces", result.len());
    Ok(result)
}

pub fn get_primary_interface() -> Result<NetworkInterface, NetworkError> {
    let interfaces = enumerate_network_interfaces()?;
    
    // Find the best interface for UPnP discovery
    for interface in interfaces {
        if !interface.is_loopback && interface.supports_multicast {
            return Ok(interface);
        }
    }
    
    Err(NetworkError::NoValidInterfaces)
}

pub fn test_interface_multicast(interface: &NetworkInterface) -> bool {
    log::debug!(target: "mop::net", "Testing multicast capability for {}", interface.name);
    if interface.is_loopback || !interface.supports_multicast {
        log::debug!(target: "mop::net", "Interface {} skipped: loopback={} multicast={}",
            interface.name, interface.is_loopback, interface.supports_multicast);
        return false;
    }

    // Use the raw SSDP test function to verify multicast capability
    match crate::upnp_ssdp::test_multicast_capability() {
        Ok(_) => {
            log::info!(target: "mop::net", "Multicast test passed for {}", interface.name);
            true
        }
        Err(e) => {
            log::warn!(target: "mop::net", "Multicast test failed for {}: {:?}", interface.name, e);
            false
        }
    }
}

pub fn format_interface_info(interface: &NetworkInterface) -> String {
    let mut info = format!("{} ({})", interface.name, interface.ip);
    
    if interface.is_loopback {
        info.push_str(" [loopback]");
    }
    
    if !interface.supports_multicast {
        info.push_str(" [no multicast]");
    }
    
    match interface.has_upnp_devices {
        Some(true) => info.push_str(" [has UPnP devices]"),
        Some(false) => info.push_str(" [no UPnP devices]"),
        None => info.push_str(" [not tested]"),
    }
    
    info
}

pub fn get_local_network_range(interface: &NetworkInterface) -> Option<String> {
    let ip = interface.ip;
    let octets = ip.octets();
    
    // Assume /24 network for common home networks
    // This is a simplification but works for most cases
    if is_private_ip(&ip) {
        Some(format!("{}.{}.{}", octets[0], octets[1], octets[2]))
    } else {
        None
    }
}

fn is_private_ip(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    match octets[0] {
        10 => true,
        172 => octets[1] >= 16 && octets[1] <= 31,
        192 => octets[1] == 168,
        _ => false,
    }
}