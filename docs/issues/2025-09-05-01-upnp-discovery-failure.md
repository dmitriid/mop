# UPnP Discovery Failure Analysis - Initial

## Problem Statement

**CRITICAL**: User confirms UPnP devices ARE present on network (Sonos speaker, Plex server) and VLC discovers them in ~5 seconds. However, MOP app finds no devices and doesn't trigger macOS network permission dialog.

This completely invalidates previous assumption that "no UPnP devices exist" - there's an actual discovery failure in the app.

## Evidence

1. **UPnP devices confirmed present**: Sonos speaker + Plex server on network
2. **VLC discovers them quickly**: ~5 seconds discovery time
3. **MOP finds nothing**: App reports no devices found
4. **No permission dialog**: macOS doesn't prompt for network access

## Code Analysis

### Discovery Flow (src/upnp.rs)
```
main.rs:30 → app.start_discovery() → upnp::start_discovery() → 
std::thread::spawn → tokio::Runtime::new() → progressive_discovery() →
test_network_access() → simple_ssdp_discovery() → upnp_client::discover_pnp_locations()
```

### upnp-client Library Operations (from source)
```rust
// discovery.rs:20-30
let socket = UdpSocket::bind([0,0,0,0]:0).await?;
socket.join_multicast_v4(239.255.255.250, 0.0.0.0)?;
socket.send_to(DISCOVERY_REQUEST, [239.255.255.250]:1900).await?;
// Then listens for responses in stream
```

### MOP's Stream Processing (src/upnp.rs:134-173)
```rust
let discovery_result = upnp_client::discovery::discover_pnp_locations().await;
match discovery_result {
    Ok(stream) => {
        // 10 second collection timeout
        // 500ms timeout per device
        // Max 20 devices
        // Early exit after 5 seconds if no devices
    }
}
```

## Root Cause Hypotheses

### 1. **Network Interface Binding Issue**
- **upnp-client binds to 0.0.0.0:0** (any interface, any port)
- **macOS might be binding to wrong interface** (e.g., loopback instead of WiFi)
- **VLC might use different binding strategy**

### 2. **Tokio Runtime Context Issue**  
- **MOP creates runtime in std::thread** - potential context isolation
- **VLC likely uses different network stack** (native/Qt/C++)
- **Runtime binding might inherit thread's network context incorrectly**

### 3. **Multicast Group Join Failure**
- **join_multicast_v4() might fail silently** on specific network configurations
- **No error checking on multicast join in upnp-client**
- **Interface selection for multicast might be wrong**

### 4. **Response Processing Issue**
- **Stream processing times out too quickly** (500ms per device)
- **Response parsing might fail** on actual device responses
- **Buffer size issues** with real device responses vs expected format

### 5. **Network Permission Context**
- **Background thread inherits different permissions** than main thread
- **CLI app permissions vs GUI app permissions**
- **Tokio socket creation in thread context** doesn't trigger permission checks

## Key Issues from Previous Analyses

Previous Plan 3 had good insights about:
- **Error reporting inadequacy** - need detailed error exposure
- **Timeout configuration** - might be too aggressive  
- **Network testing inconsistency** - tests different stack than actual discovery

## Discovery Strategy Issues

Current strategy (UPnP → port scan fallback) assumes UPnP will work. Since VLC finds devices quickly but MOP doesn't, this suggests:
- **MOP's UPnP implementation has bugs**
- **Not a "no devices exist" problem**
- **Not a "UPnP disabled" problem**

## Next Steps Required

1. **Add detailed error logging** to every step of discovery chain
2. **Test multicast operations** using exact same patterns as upnp-client
3. **Verify network interface selection** for multicast binding
4. **Compare network behavior** between working VLC and failing MOP
5. **Test permission dialog triggers** with explicit network operations