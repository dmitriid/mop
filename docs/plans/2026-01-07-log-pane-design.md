# Log Pane Design

A togglable debug log pane for diagnosing UPnP discovery issues across different Linux distributions.

## Data Model

```rust
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub category: LogCategory,
    pub severity: LogSeverity,
    pub message: String,
}

pub enum LogCategory {
    Net,      // [NET]  - Socket ops, multicast, interfaces
    Disc,     // [DISC] - Discovery phases, device found
    Soap,     // [SOAP] - UPnP SOAP requests/responses
    Http,     // [HTTP] - HTTP requests, responses, timeouts
    Xml,      // [XML]  - Parsing operations
    App,      // [APP]  - General app events
}

pub enum LogSeverity {
    Error,    // Bold red
    Warn,     // Yellow
    Info,     // Default/white
    Debug,    // Dim gray
    Trace,    // Very dim, for packet dumps etc.
}
```

**Ring Buffer:** Fixed capacity of 2000 entries. Thread-safe via `Arc<Mutex<VecDeque<LogEntry>>>`.

## UI Layout

### Three-State Toggle

Press `l` to cycle states. Press `Esc` to close from any state.

**State 0: OFF (default)**
```
┌─────────────────────────────────────────────┐
│  Server List          │  Server Info        │
│                       │                     │
│                       │                     │
│                       │                     │
└─────────────────────────────────────────────┘
```

**State 1: BOTTOM PANE (first `l` press) - 35% height**
```
┌─────────────────────────────────────────────┐
│  Server List          │  Server Info        │
│                       │                     │
├─────────────────────────────────────────────┤
│ 12:34:56 [NET] Binding socket 0.0.0.0:0     │
│ 12:34:56 [NET] Joining multicast 239.255...  │
│ 12:34:57 [DISC] Phase 1 starting...         │
│ ─────────────────────────────────────────── │
│ Filter: ssdp█         [s]ave  [/]filter     │
└─────────────────────────────────────────────┘
```

**State 2: FULLSCREEN (second `l` press)**
```
┌─────────────────────────────────────────────┐
│ 12:34:56 [NET] Binding socket 0.0.0.0:0     │
│ 12:34:56 [NET] Joining multicast 239.255... │
│ 12:34:56 [NET] Setting read timeout 100ms   │
│ 12:34:57 [DISC] Phase 1 starting...         │
│ ... (full scrollable history)               │
│ ─────────────────────────────────────────── │
│ Filter: █              [s]ave  [/]filter    │
└─────────────────────────────────────────────┘
```

## Key Bindings (in log pane)

| Key | Action |
|-----|--------|
| `l` | Cycle to next state (off → bottom → fullscreen → off) |
| `Esc` | Close log pane from any state |
| `↑/↓` or `j/k` | Scroll one line |
| `t` | Jump to top |
| `b` | Jump to bottom (re-enables auto-scroll) |
| `PageUp/PageDown` | Scroll by page |
| `/` | Enter filter mode |
| `s` | Export logs to file |

## Visual Styling

### Category Colors

| Category | Color | Example |
|----------|-------|---------|
| `[NET]` | Cyan | Socket ops, multicast joins |
| `[DISC]` | Green | Discovery phases, devices found |
| `[SOAP]` | Magenta | UPnP SOAP requests/responses |
| `[HTTP]` | Blue | HTTP requests, status codes |
| `[XML]` | Yellow | Parsing events |
| `[APP]` | White | General app events |

### Severity Modifiers

| Severity | Style |
|----------|-------|
| Error | Bold + Red foreground (overrides category) |
| Warn | Yellow foreground (overrides category) |
| Info | Normal intensity |
| Debug | Dim intensity |
| Trace | Very dim + italic |

### Log Line Format

```
HH:MM:SS [CAT] message text here
```

Example:
```
12:34:56 [NET] Binding UDP socket to 0.0.0.0:0          (cyan, normal)
12:34:56 [NET] Joining multicast 239.255.255.250        (cyan, normal)
12:34:56 [NET] ERROR: Permission denied on multicast    (red, bold)
12:34:57 [DISC] Phase 1: rupnp discovery starting       (green, normal)
```

## Filtering

Press `/` to enter filter mode.

- Typing filters logs in real-time (case-insensitive substring match)
- Filter applies to full line (timestamp, category, message)
- `Enter` - Confirm filter, exit filter mode (filter persists)
- `Esc` - Cancel, restore previous filter
- Empty filter + `Enter` - Clear filter, show all

### Filter Examples

| Filter | Shows |
|--------|-------|
| `net` | All `[NET]` entries |
| `error` | All error messages |
| `multicast` | Lines containing "multicast" |
| `plex` | Device discovery mentioning Plex |

### Scroll Behavior

- New logs appear at bottom
- Auto-scroll when at bottom; manual scroll disables it
- `b` re-enables auto-scroll
- Scroll position preserved across toggle states
- Filtered view shows "Showing X of Y entries"

## File Export

Press `s` to export.

**Path:** `~/.cache/mop/debug-YYYY-MM-DD-HHMMSS.log`

**Format:**
```
MOP Debug Log - Exported 2026-01-07 14:23:45
Filter: (none)
Entries: 847

---
14:20:01 [NET]   INFO  Binding UDP socket to 0.0.0.0:0
14:20:01 [NET]   INFO  Joining multicast 239.255.255.250
14:20:01 [NET]   ERROR Permission denied on multicast join
14:20:02 [DISC]  INFO  Phase 1: rupnp discovery starting
```

- Exports all logs (ignores filter)
- Creates directory if needed
- Shows confirmation message in log pane

## Logger Infrastructure

Uses the `log` crate with a custom backend:

```rust
pub struct RingBufferLogger {
    buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    capacity: usize,
}

impl log::Log for RingBufferLogger { ... }
```

### Initialization

```rust
static LOGGER: OnceLock<RingBufferLogger> = OnceLock::new();

fn main() {
    let logger = RingBufferLogger::new(2000);
    let buffer = logger.buffer_handle();
    LOGGER.set(logger).unwrap();
    log::set_logger(LOGGER.get().unwrap()).unwrap();
    log::set_max_level(LevelFilter::Trace);

    let app = App::new(buffer);
}
```

### Target-to-Category Mapping

| Target pattern | Category |
|----------------|----------|
| `mop::net`, `net` | NET |
| `mop::upnp`, `mop::discovery`, `rupnp` | DISC |
| `mop::soap` | SOAP |
| `reqwest`, `mop::http` | HTTP |
| `mop::xml`, `quick_xml` | XML |
| Everything else | APP |

## Instrumentation Points

### Network (NET)
- Interface enumeration: name, IP, multicast capability
- Socket creation: bind address, result
- Multicast join: interface, group, result
- Socket options: timeouts, buffer sizes
- Send/receive: bytes, timeouts

### Discovery (DISC)
- Phase transitions
- M-SEARCH sent: target, destination
- SSDP response: source IP, location, server
- Device description fetch: URL, result
- rupnp events

### SOAP
- Request: URL, action, body size
- Response: status, body size, parse result

### HTTP
- Request: method, URL
- Response: status, content-type, size
- Errors: timeout, connection refused, DNS

### XML
- Parse start: document type, size
- Parse result: success/error
