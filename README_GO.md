# MOP - Go Version

A Terminal User Interface (TUI) application written in Go for discovering and exploring UPnP services, specifically targeting Plex servers. This is a Go implementation using the BubbleTea TUI library.

## Features

- **UPnP Discovery**: SSDP-based discovery of Plex servers and media devices
- **Directory Browsing**: Navigate through media server directories using UPnP ContentDirectory service
- **TUI Interface**: Terminal-based interface with server list, directory browser, and file details
- **Configuration**: TOML-based config for player command and close-on-run behavior
- **File Playback**: Execute external player commands on media files

## Dependencies

- Go 1.21 or later
- BubbleTea TUI library
- TOML configuration support
- XML parsing for UPnP responses

## Building

```bash
# Build the application
./build_go.sh

# Or manually:
go mod tidy
go build -o mop-go .
```

## Running

```bash
./mop-go
```

## Configuration

The application uses a TOML configuration file at `~/.config/mop.toml`:

```toml
[mop]
run = "mpv"
close_on_run = true
```

- `run`: Command to execute when playing media files (default: "mpv")
- `close_on_run`: Whether to close MOP when starting playback (default: true)

## Controls

### Server List
- `↑/↓` or `j/k`: Navigate servers
- `Enter`: Select server
- `?`: Show help
- `,`: Show settings
- `e`: Copy errors to clipboard (if any)
- `q`: Quit

### Directory Browser
- `↑/↓` or `j/k`: Navigate items
- `Enter`: Open directory or play file
- `Backspace`: Go back
- `?`: Show help
- `,`: Show settings
- `q`: Quit

### Settings
- `e`: Edit current field
- `Tab`: Switch between fields
- `Enter`: Save changes
- `Esc`: Cancel editing
- `,`: Close settings

## Architecture

### Key Components

1. **main.go**: Application entry point
2. **app.go**: Main application logic and state management
3. **ui.go**: TUI rendering using BubbleTea
4. **upnp.go**: UPnP discovery and directory browsing
5. **config.go**: Configuration management
6. **types.go**: Type definitions

### UPnP Discovery

The application uses two discovery methods:

1. **SSDP Discovery**: Sends M-SEARCH requests to discover UPnP devices
2. **Port Scanning**: Fallback method that scans common media server ports

### Directory Browsing

1. **UPnP ContentDirectory**: Primary method using SOAP requests
2. **HTTP Fallback**: Direct HTTP requests to media server APIs

## Differences from Rust Version

- Uses BubbleTea instead of Ratatui for TUI
- Simplified error handling
- Go's built-in concurrency instead of Rust's async/await
- Different XML parsing approach
- Streamlined configuration management

## Error Handling

The application provides comprehensive error reporting:

- Discovery errors are collected and can be copied to clipboard
- Network timeouts are handled gracefully
- UPnP service failures fall back to HTTP browsing
- Configuration errors fall back to defaults

## Performance

- Non-blocking discovery process
- Efficient directory browsing with caching
- Minimal memory footprint
- Fast startup time

## Troubleshooting

1. **No devices found**: Check network connectivity and UPnP support
2. **Permission denied**: Ensure proper network permissions
3. **Player not working**: Verify the player command in configuration
4. **Directory browsing fails**: Check if UPnP services are available

## Development

To modify or extend the application:

1. Follow Go best practices
2. Maintain the TUI interface consistency
3. Test with various UPnP devices
4. Ensure proper error handling
5. Update documentation as needed
