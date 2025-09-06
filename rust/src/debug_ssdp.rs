use std::net::{UdpSocket, SocketAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use std::io::ErrorKind;

pub fn debug_ssdp_discovery() {
    println!("🔍 SSDP Debug Tool");
    println!("==================");
    
    // Step 1: Test basic UDP socket creation
    println!("\n1. Testing UDP socket creation...");
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => {
            println!("✅ UDP socket created successfully");
            println!("   Local address: {:?}", s.local_addr());
            s
        },
        Err(e) => {
            println!("❌ Failed to create UDP socket: {}", e);
            return;
        }
    };
    
    // Step 2: Test socket configuration
    println!("\n2. Configuring socket timeouts...");
    if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(100))) {
        println!("❌ Failed to set read timeout: {}", e);
        return;
    }
    if let Err(e) = socket.set_write_timeout(Some(Duration::from_millis(1000))) {
        println!("❌ Failed to set write timeout: {}", e);
        return;
    }
    println!("✅ Socket timeouts configured");
    
    // Step 3: Test multicast join
    println!("\n3. Joining multicast group...");
    let multicast_ip = Ipv4Addr::new(239, 255, 255, 250);
    let interface_ip = Ipv4Addr::new(0, 0, 0, 0);
    
    match socket.join_multicast_v4(&multicast_ip, &interface_ip) {
        Ok(_) => println!("✅ Successfully joined multicast group 239.255.255.250"),
        Err(e) => {
            println!("❌ Failed to join multicast group: {}", e);
            match e.kind() {
                ErrorKind::PermissionDenied => {
                    println!("   This is likely a macOS Local Network permission issue");
                    println!("   Go to: System Preferences > Security & Privacy > Privacy > Local Network");
                    println!("   Find this application and check the box");
                }
                _ => println!("   Error details: {:?}", e),
            }
            return;
        }
    }
    
    // Step 4: Send SSDP discovery message
    println!("\n4. Sending SSDP M-SEARCH request...");
    let search_request = "M-SEARCH * HTTP/1.1\r\n\
                         HOST: 239.255.255.250:1900\r\n\
                         MAN: \"ssdp:discover\"\r\n\
                         ST: upnp:rootdevice\r\n\
                         MX: 3\r\n\r\n";
    
    let multicast_addr: SocketAddr = "239.255.255.250:1900".parse().unwrap();
    
    match socket.send_to(search_request.as_bytes(), multicast_addr) {
        Ok(bytes_sent) => {
            println!("✅ SSDP request sent successfully ({} bytes)", bytes_sent);
            println!("   Request content:");
            for line in search_request.lines() {
                println!("   > {}", line);
            }
        },
        Err(e) => {
            println!("❌ Failed to send SSDP request: {}", e);
            match e.kind() {
                ErrorKind::PermissionDenied => {
                    println!("   Permission denied - multicast sending blocked");
                }
                _ => println!("   Error details: {:?}", e),
            }
            return;
        }
    }
    
    // Step 5: Listen for responses
    println!("\n5. Listening for SSDP responses (15 seconds)...");
    let start_time = Instant::now();
    let listen_duration = Duration::from_secs(15);
    let mut response_count = 0;
    let mut unique_devices = std::collections::HashSet::new();
    
    while start_time.elapsed() < listen_duration {
        let mut buf = [0; 4096];
        match socket.recv_from(&mut buf) {
            Ok((size, addr)) => {
                response_count += 1;
                println!("\n📨 Response #{} from {}", response_count, addr);
                
                if let Ok(response) = std::str::from_utf8(&buf[..size]) {
                    // Parse basic info
                    let mut location = None;
                    let mut server = None;
                    let mut usn = None;
                    
                    for line in response.lines() {
                        let line = line.trim();
                        if let Some(colon_pos) = line.find(':') {
                            let (header, value) = line.split_at(colon_pos);
                            let header = header.trim().to_lowercase();
                            let value = value[1..].trim();
                            
                            match header.as_str() {
                                "location" => location = Some(value),
                                "server" => server = Some(value),
                                "usn" => usn = Some(value),
                                _ => {}
                            }
                        }
                    }
                    
                    if let Some(loc) = location {
                        unique_devices.insert(loc.to_string());
                        println!("   Location: {}", loc);
                    }
                    if let Some(srv) = server {
                        println!("   Server: {}", srv);
                    }
                    if let Some(u) = usn {
                        println!("   USN: {}", u);
                    }
                    
                    // Show first few lines of response
                    println!("   Response preview:");
                    for (i, line) in response.lines().take(5).enumerate() {
                        println!("   {:2}: {}", i+1, line);
                    }
                    if response.lines().count() > 5 {
                        println!("   ... ({} more lines)", response.lines().count() - 5);
                    }
                } else {
                    println!("   [Binary response - {} bytes]", size);
                }
            },
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                // Normal timeout, continue listening
                continue;
            },
            Err(e) => {
                println!("❌ Error receiving response: {}", e);
                break;
            }
        }
    }
    
    // Step 6: Summary
    println!("\n📊 Discovery Summary");
    println!("====================");
    println!("Total responses received: {}", response_count);
    println!("Unique devices found: {}", unique_devices.len());
    
    if unique_devices.is_empty() {
        println!("\n❌ No UPnP devices discovered");
        println!("Possible causes:");
        println!("  • macOS Local Network permission not granted");
        println!("  • No UPnP devices on network");
        println!("  • Firewall blocking multicast traffic");
        println!("  • Wrong network interface selected");
    } else {
        println!("\n✅ UPnP devices found:");
        for (i, device) in unique_devices.iter().enumerate() {
            println!("  {}. {}", i+1, device);
        }
    }
    
    println!("\n💡 If VLC finds devices but this tool doesn't, try:");
    println!("  • Grant Local Network permission in System Preferences");
    println!("  • Run with sudo (temporary test): sudo cargo run");
    println!("  • Check if VLC uses a different discovery method");
}

// Test different multicast approaches
pub fn test_multicast_methods() {
    println!("\n🧪 Testing different multicast approaches...");
    
    // Method 1: Broadcast instead of multicast
    println!("\n1. Testing broadcast discovery...");
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        socket.set_broadcast(true).ok();
        let broadcast_addr = "255.255.255.255:1900";
        let search_request = "M-SEARCH * HTTP/1.1\r\n\
                             HOST: 255.255.255.255:1900\r\n\
                             MAN: \"ssdp:discover\"\r\n\
                             ST: upnp:rootdevice\r\n\
                             MX: 1\r\n\r\n";
        
        match socket.send_to(search_request.as_bytes(), broadcast_addr) {
            Ok(_) => println!("✅ Broadcast request sent"),
            Err(e) => println!("❌ Broadcast failed: {}", e),
        }
    }
    
    // Method 2: Direct device probing
    println!("\n2. Testing direct device probing...");
    test_direct_device_probe();
}

fn test_direct_device_probe() {
    // Try to connect to common Plex/media server ports
    let test_ips = vec!["192.168.1.1", "192.168.1.100", "192.168.1.200", "192.168.0.1", "192.168.0.100"];
    let test_ports = vec![32400, 8096, 8920]; // Plex, Jellyfin, Emby
    
    for ip in test_ips {
        for port in &test_ports {
            let addr = format!("{}:{}", ip, port);
            match std::net::TcpStream::connect_timeout(
                &addr.parse().unwrap(), 
                Duration::from_millis(500)
            ) {
                Ok(_) => println!("✅ Found service at {}", addr),
                Err(_) => {}, // Silent failure
            }
        }
    }
}