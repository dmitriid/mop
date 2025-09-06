#[cfg(target_os = "macos")]
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionState {
    Granted,
    Denied,
    Unknown,
    NeedsRequest,
}

#[derive(Debug)]
pub enum PermissionError {
    Denied,
    SystemError(String),
    UserAborted,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::Denied => write!(f, "Local network permission denied"),
            PermissionError::SystemError(e) => write!(f, "System error: {}", e),
            PermissionError::UserAborted => write!(f, "User aborted permission request"),
        }
    }
}

impl std::error::Error for PermissionError {}

#[cfg(target_os = "macos")]
pub fn check_local_network_permission() -> PermissionState {
    match crate::upnp_ssdp::test_multicast_capability() {
        Ok(_) => PermissionState::Granted,
        Err(crate::upnp_ssdp::DiscoveryError::PermissionDenied) => PermissionState::Denied,
        Err(_) => PermissionState::Unknown,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn check_local_network_permission() -> PermissionState {
    // On non-macOS systems, assume permission is granted
    PermissionState::Granted
}

#[cfg(target_os = "macos")]
pub fn request_permission_interactive() -> Result<PermissionState, PermissionError> {
    println!("\n🔒 macOS Local Network Permission Required");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("This application needs permission to discover UPnP devices on your local network.");
    println!("This allows finding Plex servers, Sonos speakers, and other media devices.");
    println!();
    println!("📱 Steps to grant permission:");
    println!("   1. A permission dialog should appear - click 'Allow'");
    println!("   2. If no dialog appears:");
    println!("      • Open System Preferences > Security & Privacy > Privacy");
    println!("      • Select 'Local Network' from the list");
    println!("      • Find this application and check the box");
    println!("   3. Restart this application after granting permission");
    println!();
    print!("Press Enter to attempt permission request, or 'q' to quit: ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        if input.trim().to_lowercase() == "q" {
            return Err(PermissionError::UserAborted);
        }
    }
    
    // Attempt to trigger permission dialog
    match trigger_permission_dialog() {
        Ok(_) => {
            println!("✅ Permission request sent. Checking status...");
            
            // Wait a moment for user to respond to dialog
            std::thread::sleep(Duration::from_secs(1));
            
            // Check if permission was granted
            match check_local_network_permission() {
                PermissionState::Granted => {
                    println!("✅ Local network permission granted!");
                    Ok(PermissionState::Granted)
                }
                PermissionState::Denied => {
                    println!("❌ Permission was denied or dialog was dismissed.");
                    println!("💡 To grant manually: System Preferences > Security & Privacy > Privacy > Local Network");
                    Err(PermissionError::Denied)
                }
                _ => {
                    println!("⚠️  Permission status unclear. You may need to grant permission manually.");
                    println!("💡 Go to: System Preferences > Security & Privacy > Privacy > Local Network");
                    Err(PermissionError::SystemError("Unclear permission state".to_string()))
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to request permission: {}", e);
            println!("💡 Please grant permission manually:");
            println!("   System Preferences > Security & Privacy > Privacy > Local Network");
            Err(PermissionError::SystemError(format!("Failed to trigger dialog: {}", e)))
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_permission_interactive() -> Result<PermissionState, PermissionError> {
    // On non-macOS systems, no permission needed
    Ok(PermissionState::Granted)
}

#[cfg(target_os = "macos")]
fn trigger_permission_dialog() -> Result<(), Box<dyn std::error::Error>> {
    use std::net::{UdpSocket, Ipv4Addr};
    
    // Create a socket and join multicast group to trigger permission dialog
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_write_timeout(Some(Duration::from_millis(1000)))?;
    
    // Join the UPnP multicast group
    let multicast_ip = Ipv4Addr::new(239, 255, 255, 250);
    let interface_ip = Ipv4Addr::new(0, 0, 0, 0);
    
    socket.join_multicast_v4(&multicast_ip, &interface_ip)?;
    
    // Send a test SSDP message to trigger the permission dialog
    let ssdp_message = "M-SEARCH * HTTP/1.1\r\n\
                       HOST: 239.255.255.250:1900\r\n\
                       MAN: \"ssdp:discover\"\r\n\
                       ST: upnp:rootdevice\r\n\
                       MX: 1\r\n\r\n";
    
    let multicast_addr: std::net::SocketAddr = "239.255.255.250:1900".parse()?;
    socket.send_to(ssdp_message.as_bytes(), multicast_addr)?;
    
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn show_permission_help() {
    println!("\n🔒 macOS Local Network Permission Help");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("If UPnP discovery is not working, you may need to grant Local Network permission:");
    println!();
    println!("📱 Manual permission steps:");
    println!("   1. Open System Preferences (or System Settings on newer macOS)");
    println!("   2. Go to Security & Privacy > Privacy");
    println!("   3. Select 'Local Network' from the left sidebar");
    println!("   4. Find this application in the list");
    println!("   5. Check the box next to the application name");
    println!("   6. Restart this application");
    println!();
    println!("🔍 Why this permission is needed:");
    println!("   • UPnP discovery uses multicast networking");
    println!("   • macOS requires explicit permission for local network access");
    println!("   • This allows finding Plex, Sonos, and other UPnP devices");
    println!();
}

#[cfg(not(target_os = "macos"))]
pub fn show_permission_help() {
    println!("Local network permissions are not required on this platform.");
}