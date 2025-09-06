package main

import (
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

var loggingSetup bool

// setupLogging configures logging to write to a file
func setupLogging() {
	if loggingSetup {
		return // Already set up
	}
	
	// Open log file next to the executable
	logFile, err := os.OpenFile("mop.log", os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0666)
	if err != nil {
		// If we can't open the log file, just use stderr
		log.SetOutput(os.Stderr)
		loggingSetup = true
		return
	}
	
	// Set log output to file
	log.SetOutput(logFile)
	log.SetFlags(log.LstdFlags | log.Lshortfile)
	loggingSetup = true
}

// SSDP discovery implementation
func DiscoverUpnpDevices() ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	setupLogging()
	log.Println("Starting UPnP discovery...")

	// Try SSDP discovery first
	log.Println("Trying SSDP discovery...")
	ssdpDevices, ssdpErrors := discoverViaSSDP()
	log.Printf("SSDP found %d devices, %d errors\n", len(ssdpDevices), len(ssdpErrors))
	devices = append(devices, ssdpDevices...)
	errors = append(errors, ssdpErrors...)

	// Try port scanning as fallback
	log.Println("Trying port scan discovery...")
	portDevices, portErrors := discoverViaPortScan()
	log.Printf("Port scan found %d devices, %d errors\n", len(portDevices), len(portErrors))
	for _, device := range portDevices {
		// Check for duplicates
		found := false
		for _, existing := range devices {
			if existing.Location == device.Location {
				found = true
				break
			}
		}
		if !found {
			devices = append(devices, device)
		}
	}
	errors = append(errors, portErrors...)

	log.Printf("Total devices found: %d\n", len(devices))
	return devices, errors
}

// Discovery with callback for real-time updates
func DiscoverUpnpDevicesWithCallback(callback func(UpnpDevice)) ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	setupLogging()
	log.Println("Starting UPnP discovery with callback...")

	// Try SSDP discovery first
	log.Println("Trying SSDP discovery...")
	ssdpDevices, ssdpErrors := discoverViaSSDPWithCallback(callback)
	log.Printf("SSDP found %d devices, %d errors\n", len(ssdpDevices), len(ssdpErrors))
	devices = append(devices, ssdpDevices...)
	errors = append(errors, ssdpErrors...)

	// Try port scanning as fallback
	log.Println("Trying port scan discovery...")
	portDevices, portErrors := discoverViaPortScanWithCallback(callback)
	log.Printf("Port scan found %d devices, %d errors\n", len(portDevices), len(portErrors))
	for _, device := range portDevices {
		// Check for duplicates
		found := false
		for _, existing := range devices {
			if existing.Location == device.Location {
				found = true
				break
			}
		}
		if !found {
			devices = append(devices, device)
		}
	}
	errors = append(errors, portErrors...)

	log.Printf("Total devices found: %d\n", len(devices))
	return devices, errors
}

func discoverViaSSDP() ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	log.Println("Creating UDP socket for SSDP...")
	// Create UDP socket
	conn, err := net.ListenPacket("udp4", ":0")
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to create UDP socket: %v", err))
		return devices, errors
	}
	defer conn.Close()

	// Set timeout
	conn.SetDeadline(time.Now().Add(5 * time.Second))

	// Send M-SEARCH request
	multicastAddr, err := net.ResolveUDPAddr("udp4", "239.255.255.250:1900")
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to resolve multicast address: %v", err))
		return devices, errors
	}

	searchRequest := "M-SEARCH * HTTP/1.1\r\n" +
		"HOST: 239.255.255.250:1900\r\n" +
		"MAN: \"ssdp:discover\"\r\n" +
		"ST: upnp:rootdevice\r\n" +
		"MX: 3\r\n\r\n"

	log.Println("Sending M-SEARCH for root devices...")
	_, err = conn.WriteTo([]byte(searchRequest), multicastAddr)
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to send M-SEARCH: %v", err))
		return devices, errors
	}

	// Also search for media devices specifically
	mediaSearch := "M-SEARCH * HTTP/1.1\r\n" +
		"HOST: 239.255.255.250:1900\r\n" +
		"MAN: \"ssdp:discover\"\r\n" +
		"ST: urn:schemas-upnp-org:device:MediaServer:1\r\n" +
		"MX: 3\r\n\r\n"

	log.Println("Sending M-SEARCH for media servers...")
	conn.WriteTo([]byte(mediaSearch), multicastAddr)

	// Collect responses
	buffer := make([]byte, 4096)
	deviceMap := make(map[string]UpnpDevice) // Use location as key to avoid duplicates

	log.Println("Waiting for SSDP responses...")
	responseCount := 0
	for {
		conn.SetDeadline(time.Now().Add(1 * time.Second))
		n, addr, err := conn.ReadFrom(buffer)
		if err != nil {
			if netErr, ok := err.(net.Error); ok && netErr.Timeout() {
				log.Printf("SSDP timeout after %d responses\n", responseCount)
				break
			}
			log.Printf("SSDP read error: %v\n", err)
			continue
		}

		responseCount++
		response := string(buffer[:n])
		log.Printf("Received SSDP response %d from %v (length: %d)\n", responseCount, addr, n)
		if device := parseSSDPResponse(response); device != nil {
			log.Printf("Parsed device: %s at %s\n", device.Name, device.Location)
			deviceMap[device.Location] = *device
		} else {
			log.Printf("Failed to parse response (first 200 chars): %s\n", response[:min(200, len(response))])
		}
	}

	// Convert map to slice
	for _, device := range deviceMap {
		devices = append(devices, device)
	}

	return devices, errors
}

func parseSSDPResponse(response string) *UpnpDevice {
	// Only process HTTP 200 OK responses
	if !strings.HasPrefix(response, "HTTP/1.1 200 OK") {
		return nil
	}

	var location, server, st, usn string

	lines := strings.Split(response, "\r\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		colonIndex := strings.Index(line, ":")
		if colonIndex == -1 {
			continue
		}

		header := strings.ToLower(strings.TrimSpace(line[:colonIndex]))
		value := strings.TrimSpace(line[colonIndex+1:])

		switch header {
		case "location":
			location = value
		case "server":
			server = value
		case "st":
			st = value
		case "usn":
			usn = value
		}
	}

	if location == "" {
		return nil
	}

	baseURL := extractBaseURL(location)
	deviceType := st
	if deviceType == "" {
		deviceType = "Unknown"
	}

	manufacturer := server
	if manufacturer == "" {
		manufacturer = "Unknown"
	}

	// Extract friendly name from server header or USN
	friendlyName := extractFriendlyName(server, usn, deviceType)
	
	displayName := friendlyName
	if manufacturer != "Unknown" {
		displayName = fmt.Sprintf("%s (%s)", friendlyName, manufacturer)
	}

	// Try to get content directory URL
	contentDirURL := getContentDirectoryURL(location)

	return &UpnpDevice{
		Name:                displayName,
		Location:            location,
		BaseURL:             baseURL,
		DeviceClient:        manufacturer,
		ContentDirectoryURL: contentDirURL,
	}
}

func extractFriendlyName(server, usn, deviceType string) string {
	// Try to extract meaningful names from server header
	if server != "" {
		// Look for common media server patterns
		if strings.Contains(strings.ToLower(server), "plex") || strings.Contains(strings.ToLower(server), "platinum") {
			return "Plex Media Server"
		}
		if strings.Contains(strings.ToLower(server), "jellyfin") {
			return "Jellyfin Server"
		}
		if strings.Contains(strings.ToLower(server), "emby") {
			return "Emby Server"
		}
		if strings.Contains(strings.ToLower(server), "sonos") {
			return "Sonos Speaker"
		}
		if strings.Contains(strings.ToLower(server), "chromecast") {
			return "Chromecast"
		}
		if strings.Contains(strings.ToLower(server), "hue") {
			return "Philips Hue Bridge"
		}
		if strings.Contains(strings.ToLower(server), "hp-ilo") {
			return "HP iLO Server"
		}
	}
	
	// Try to extract from USN
	if usn != "" {
		// Look for device names in USN
		if strings.Contains(usn, "RINCON_") {
			return "Sonos Speaker"
		}
		if strings.Contains(usn, "uuid:") {
			// Extract UUID and try to identify device type
			uuidStart := strings.Index(usn, "uuid:")
			if uuidStart != -1 {
				uuidPart := usn[uuidStart+5:]
				if uuidEnd := strings.Index(uuidPart, "::"); uuidEnd != -1 {
					uuid := uuidPart[:uuidEnd]
					// Use first 8 chars of UUID as identifier
					return fmt.Sprintf("Device %s", uuid[:min(8, len(uuid))])
				}
			}
		}
	}
	
	// Fallback to device type
	switch deviceType {
	case "urn:schemas-upnp-org:device:MediaServer:1":
		return "Media Server"
	case "upnp:rootdevice":
		return "UPnP Device"
	case "urn:schemas-upnp-org:device:basic:1":
		return "Basic Device"
	default:
		return "Unknown Device"
	}
}

func extractBaseURL(location string) string {
	// Simple URL parsing
	if strings.HasPrefix(location, "http://") {
		parts := strings.Split(location[7:], "/")
		if len(parts) > 0 {
			hostPort := parts[0]
			if !strings.Contains(hostPort, ":") {
				hostPort += ":80"
			}
			return "http://" + hostPort
		}
	}
	return location
}

func getContentDirectoryURL(location string) string {
	// Try to fetch device description to find ContentDirectory service
	client := &http.Client{Timeout: 5 * time.Second}
	resp, err := client.Get(location)
	if err != nil {
		return ""
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return ""
	}

	return parseContentDirectoryFromXML(string(body), location)
}

func parseContentDirectoryFromXML(xmlContent, baseURL string) string {
	// Simple XML parsing to find ContentDirectory service
	lines := strings.Split(xmlContent, "\n")
	var inService, inServiceType, inControlURL bool
	var currentServiceType, currentControlURL string

	for _, line := range lines {
		line = strings.TrimSpace(line)
		
		if strings.Contains(line, "<service>") {
			inService = true
			currentServiceType = ""
			currentControlURL = ""
		} else if strings.Contains(line, "</service>") {
			if inService && strings.Contains(currentServiceType, "ContentDirectory") && currentControlURL != "" {
				// Resolve relative URL
				if strings.HasPrefix(currentControlURL, "http") {
					return currentControlURL
				}
				return baseURL + currentControlURL
			}
			inService = false
		} else if inService {
			if strings.Contains(line, "<serviceType>") {
				inServiceType = true
			} else if strings.Contains(line, "</serviceType>") {
				inServiceType = false
			} else if strings.Contains(line, "<controlURL>") {
				inControlURL = true
			} else if strings.Contains(line, "</controlURL>") {
				inControlURL = false
			} else if inServiceType {
				currentServiceType = extractTextContent(line)
			} else if inControlURL {
				currentControlURL = extractTextContent(line)
			}
		}
	}

	return ""
}

func extractTextContent(line string) string {
	// Simple text extraction from XML tags
	start := strings.Index(line, ">")
	end := strings.LastIndex(line, "<")
	if start != -1 && end != -1 && start < end {
		return strings.TrimSpace(line[start+1:end])
	}
	return ""
}

func discoverViaPortScan() ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	// Get local network range
	networkBase, err := getLocalNetworkBase()
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to get local network: %v", err))
		return devices, errors
	}

	// Scan promising IPs and ports
	promisingIPs := []int{1, 2, 10, 100, 200, 254}
	mediaPorts := []int{32400, 8096, 8920} // Plex, Jellyfin, Emby

	for _, ipSuffix := range promisingIPs {
		ip := fmt.Sprintf("%s.%d", networkBase, ipSuffix)
		for _, port := range mediaPorts {
			if device := scanEndpoint(ip, port); device != nil {
				devices = append(devices, *device)
			}
		}
	}

	return devices, errors
}

func getLocalNetworkBase() (string, error) {
	// Connect to external address to get local IP
	conn, err := net.Dial("udp", "8.8.8.8:80")
	if err != nil {
		return "", err
	}
	defer conn.Close()

	localAddr := conn.LocalAddr().(*net.UDPAddr)
	ip := localAddr.IP.String()
	parts := strings.Split(ip, ".")
	if len(parts) >= 3 {
		return strings.Join(parts[:3], "."), nil
	}

	return "", fmt.Errorf("invalid IP address: %s", ip)
}

func scanEndpoint(ip string, port int) *UpnpDevice {
	url := fmt.Sprintf("http://%s:%d", ip, port)
	
	client := &http.Client{Timeout: 500 * time.Millisecond}
	
	// Try health endpoints
	endpoints := []string{"/", "/status", "/identity"}
	
	for _, endpoint := range endpoints {
		testURL := url + endpoint
		resp, err := client.Get(testURL)
		if err != nil {
			continue
		}
		resp.Body.Close()
		
		if resp.StatusCode == 200 {
			serverName := getServerName(port, ip)
			return &UpnpDevice{
				Name:                serverName,
				Location:            url,
				BaseURL:             url,
				DeviceClient:        "PortScan",
				ContentDirectoryURL: "",
			}
		}
	}
	
	return nil
}

func getServerName(port int, ip string) string {
	switch port {
	case 32400:
		return fmt.Sprintf("Plex Server (%s:%d)", ip, port)
	case 8096:
		return fmt.Sprintf("Jellyfin Server (%s:%d)", ip, port)
	case 8920:
		return fmt.Sprintf("Emby Server (%s:%d)", ip, port)
	default:
		return fmt.Sprintf("Media Server (%s:%d)", ip, port)
	}
}

// Directory browsing implementation
func BrowseDirectory(server *UpnpDevice, path []string, containerIDMap map[string]string) ([]DirectoryItem, error) {
	// Determine container ID based on path
	pathKey := strings.Join(path, "/")
	containerID, exists := containerIDMap[pathKey]
	if !exists {
		containerID = "0" // Root container
	}

	// Try UPnP ContentDirectory service first
	if server.ContentDirectoryURL != "" {
		items, err := browseUpnpContentDirectory(server.ContentDirectoryURL, containerID)
		if err == nil {
			// Update container ID mapping for discovered containers
			for _, item := range items {
				if item.IsDirectory {
					newPathKey := strings.Join(append(path, item.Name), "/")
					// For now, use a simple ID generation
					containerIDMap[newPathKey] = item.Name + "_container"
				}
			}
			return items, nil
		}
	}

	// Fallback to HTTP browsing
	return browseHTTPDirectory(server.BaseURL, path)
}

func browseUpnpContentDirectory(contentDirURL, containerID string) ([]DirectoryItem, error) {
	client := &http.Client{Timeout: 10 * time.Second}

	// SOAP request for UPnP ContentDirectory Browse action
	soapBody := fmt.Sprintf(`<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
    <s:Body>
        <u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
            <ObjectID>%s</ObjectID>
            <BrowseFlag>BrowseDirectChildren</BrowseFlag>
            <Filter>*</Filter>
            <StartingIndex>0</StartingIndex>
            <RequestedCount>100</RequestedCount>
            <SortCriteria></SortCriteria>
        </u:Browse>
    </s:Body>
</s:Envelope>`, containerID)

	req, err := http.NewRequest("POST", contentDirURL, strings.NewReader(soapBody))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "text/xml; charset=utf-8")
	req.Header.Set("SOAPAction", "urn:schemas-upnp-org:service:ContentDirectory:1#Browse")
	req.Header.Set("User-Agent", "MOP/1.0")

	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("UPnP SOAP request failed with status: %d", resp.StatusCode)
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	// Check for SOAP faults
	bodyStr := string(body)
	if strings.Contains(bodyStr, "soap:Fault") || strings.Contains(bodyStr, "SOAP-ENV:Fault") {
		return nil, fmt.Errorf("UPnP SOAP fault in response")
	}

	return parseDIDLResponse(bodyStr)
}

func parseDIDLResponse(xmlContent string) ([]DirectoryItem, error) {
	// Simple XML parsing for DIDL-Lite content
	var items []DirectoryItem
	
	lines := strings.Split(xmlContent, "\n")
	var inItem, inContainer, inTitle, inResource bool
	var currentItem DirectoryItem
	var currentTitle, currentResource string

	for _, line := range lines {
		line = strings.TrimSpace(line)
		
		if strings.Contains(line, "<item") {
			inItem = true
			currentItem = DirectoryItem{IsDirectory: false}
		} else if strings.Contains(line, "<container") {
			inContainer = true
			currentItem = DirectoryItem{IsDirectory: true}
		} else if strings.Contains(line, "</item>") {
			if inItem && currentTitle != "" {
				currentItem.Name = currentTitle
				if currentResource != "" {
					currentItem.URL = currentResource
				}
				items = append(items, currentItem)
			}
			inItem = false
			currentTitle = ""
			currentResource = ""
		} else if strings.Contains(line, "</container>") {
			if inContainer && currentTitle != "" {
				currentItem.Name = currentTitle
				items = append(items, currentItem)
			}
			inContainer = false
			currentTitle = ""
		} else if strings.Contains(line, "<dc:title>") {
			inTitle = true
		} else if strings.Contains(line, "</dc:title>") {
			inTitle = false
		} else if strings.Contains(line, "<res") {
			inResource = true
		} else if strings.Contains(line, "</res>") {
			inResource = false
		} else if inTitle {
			currentTitle = extractTextContent(line)
		} else if inResource {
			currentResource = extractTextContent(line)
		}
	}

	return items, nil
}

func browseHTTPDirectory(baseURL string, path []string) ([]DirectoryItem, error) {
	var items []DirectoryItem
	client := &http.Client{Timeout: 5 * time.Second}

	// Try common media server endpoints
	var endpoints []string
	if len(path) == 0 {
		endpoints = []string{
			"/library/sections", // Plex
			"/web/index.html",   // Plex web
			"/Users",            // Jellyfin/Emby users
			"/Items",            // Jellyfin/Emby items
			"/",                 // Root directory
		}
	} else {
		endpoints = []string{"/" + strings.Join(path, "/")}
	}

	for _, endpoint := range endpoints {
		url := baseURL + endpoint
		resp, err := client.Get(url)
		if err != nil {
			continue
		}
		defer resp.Body.Close()

		if resp.StatusCode == 200 {
			body, err := io.ReadAll(resp.Body)
			if err != nil {
				continue
			}

			// Try to parse as JSON (modern media servers)
			if jsonItems := parseJSONDirectory(string(body)); len(jsonItems) > 0 {
				items = append(items, jsonItems...)
				break
			}

			// Try to parse as HTML directory listing
			if htmlItems := parseHTMLDirectory(string(body), url); len(htmlItems) > 0 {
				items = append(items, htmlItems...)
				break
			}
		}
	}

	if len(items) == 0 {
		return nil, fmt.Errorf("no browsable content found")
	}

	return items, nil
}

func parseJSONDirectory(jsonText string) []DirectoryItem {
	var items []DirectoryItem

	// Simple JSON parsing for basic structures
	if strings.Contains(jsonText, "\"MediaContainer\"") {
		// Plex-style response
		items = append(items, DirectoryItem{
			Name:        "Plex Media Server",
			IsDirectory: true,
		})
	} else if strings.Contains(jsonText, "\"Items\"") {
		// Jellyfin/Emby-style response
		items = append(items, DirectoryItem{
			Name:        "Media Library",
			IsDirectory: true,
		})
	}

	return items
}

func parseHTMLDirectory(htmlText, baseURL string) []DirectoryItem {
	var items []DirectoryItem

	// Simple HTML parsing for directory listings
	lines := strings.Split(htmlText, "\n")
	for _, line := range lines {
		if strings.Contains(line, "<a href=") && !strings.Contains(line, "Parent Directory") {
			start := strings.Index(line, "href=\"")
			if start == -1 {
				continue
			}
			start += 6
			end := strings.Index(line[start:], "\"")
			if end == -1 {
				continue
			}
			href := line[start : start+end]

			// Extract filename from href
			name := strings.TrimPrefix(strings.TrimSuffix(href, "/"), "/")
			if name == "" || name == ".." {
				continue
			}

			isDirectory := strings.HasSuffix(href, "/")
			fullURL := href
			if !strings.HasPrefix(href, "http") {
				fullURL = baseURL + href
			}

			items = append(items, DirectoryItem{
				Name:        name,
				IsDirectory: isDirectory,
				URL:         fullURL,
			})
		}
	}

	return items
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func discoverViaSSDPWithCallback(callback func(UpnpDevice)) ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	log.Println("Creating UDP socket for SSDP...")
	// Create UDP socket
	conn, err := net.ListenPacket("udp4", ":0")
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to create UDP socket: %v", err))
		return devices, errors
	}
	defer conn.Close()

	// Set timeout
	conn.SetDeadline(time.Now().Add(5 * time.Second))

	// Send M-SEARCH request
	multicastAddr, err := net.ResolveUDPAddr("udp4", "239.255.255.250:1900")
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to resolve multicast address: %v", err))
		return devices, errors
	}

	searchRequest := "M-SEARCH * HTTP/1.1\r\n" +
		"HOST: 239.255.255.250:1900\r\n" +
		"MAN: \"ssdp:discover\"\r\n" +
		"ST: upnp:rootdevice\r\n" +
		"MX: 3\r\n\r\n"

	log.Println("Sending M-SEARCH for root devices...")
	_, err = conn.WriteTo([]byte(searchRequest), multicastAddr)
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to send M-SEARCH: %v", err))
		return devices, errors
	}

	// Also search for media devices specifically
	mediaSearch := "M-SEARCH * HTTP/1.1\r\n" +
		"HOST: 239.255.255.250:1900\r\n" +
		"MAN: \"ssdp:discover\"\r\n" +
		"ST: urn:schemas-upnp-org:device:MediaServer:1\r\n" +
		"MX: 3\r\n\r\n"

	log.Println("Sending M-SEARCH for media servers...")
	conn.WriteTo([]byte(mediaSearch), multicastAddr)

	// Collect responses
	buffer := make([]byte, 4096)
	deviceMap := make(map[string]UpnpDevice) // Use location as key to avoid duplicates

	log.Println("Waiting for SSDP responses...")
	responseCount := 0
	for {
		conn.SetDeadline(time.Now().Add(1 * time.Second))
		n, addr, err := conn.ReadFrom(buffer)
		if err != nil {
			if netErr, ok := err.(net.Error); ok && netErr.Timeout() {
				log.Printf("SSDP timeout after %d responses\n", responseCount)
				break
			}
			log.Printf("SSDP read error: %v\n", err)
			continue
		}

		responseCount++
		response := string(buffer[:n])
		log.Printf("Received SSDP response %d from %v (length: %d)\n", responseCount, addr, n)
		if device := parseSSDPResponse(response); device != nil {
			log.Printf("Parsed device: %s at %s\n", device.Name, device.Location)
			// Check for duplicates
			if _, exists := deviceMap[device.Location]; !exists {
				deviceMap[device.Location] = *device
				devices = append(devices, *device)
				// Call callback immediately
				callback(*device)
			}
		} else {
			log.Printf("Failed to parse response (first 200 chars): %s\n", response[:min(200, len(response))])
		}
	}

	return devices, errors
}

func discoverViaPortScanWithCallback(callback func(UpnpDevice)) ([]UpnpDevice, []string) {
	var devices []UpnpDevice
	var errors []string

	// Get local network range
	networkBase, err := getLocalNetworkBase()
	if err != nil {
		errors = append(errors, fmt.Sprintf("Failed to get local network: %v", err))
		return devices, errors
	}

	// Scan promising IPs and ports
	promisingIPs := []int{1, 2, 10, 100, 200, 254}
	mediaPorts := []int{32400, 8096, 8920} // Plex, Jellyfin, Emby

	for _, ipSuffix := range promisingIPs {
		ip := fmt.Sprintf("%s.%d", networkBase, ipSuffix)
		for _, port := range mediaPorts {
			if device := scanEndpoint(ip, port); device != nil {
				// Check for duplicates
				found := false
				for _, existing := range devices {
					if existing.Location == device.Location {
						found = true
						break
					}
				}
				if !found {
					devices = append(devices, *device)
					callback(*device)
				}
			}
		}
	}

	return devices, errors
}
