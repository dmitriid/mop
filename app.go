package main

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

func NewApp() *App {
	config, err := LoadConfig()
	if err != nil {
		// Use default config if loading fails
		config = DefaultConfig()
	}

	app := &App{
		state:             StateServerList,
		servers:           []UpnpDevice{},
		selectedServer:    -1,
		currentDirectory:  []string{},
		directoryContents: []DirectoryItem{},
		selectedItem:      -1,
		statusMessage:     "",
		lastError:         "",
		discoveryErrors:   []string{},
		isDiscovering:     false,
		showHelp:          false,
		showSettings:      false,
		settingsEditing:   false,
		settingsField:     FieldPlayer,
		settingsInput:     "",
		containerIDMap:    make(map[string]string),
		config:            config,
		discoveryChan:     make(chan DiscoveryMessage, 100),
	}

	// Initialize with root container ID
	app.containerIDMap[""] = "0"
	
	return app
}

func (a *App) Init() tea.Cmd {
	return tea.Batch(
		a.checkDiscoveryUpdates(),
		a.tick(),
		a.periodicDiscovery(),
		a.startDiscoveryDelayed(),
	)
}

func (a *App) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		a.width = msg.Width
		a.height = msg.Height
		return a, nil
	case tea.KeyMsg:
		return a.handleKeyPress(msg)
	case DiscoveryMessage:
		return a.handleDiscoveryMessage(msg)
	case tickMsg:
		return a, tea.Batch(a.tick(), a.checkDiscoveryUpdates())
	}
	return a, nil
}

func (a *App) handleKeyPress(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q", "ctrl+c":
		return a, tea.Quit
	case "?":
		a.toggleHelp()
		return a, nil
	case ",":
		a.toggleSettings()
		return a, nil
	case "e":
		if a.hasErrors() {
			a.copyErrorsToClipboard()
		}
		return a, nil
	case "up", "k":
		a.previous()
		return a, nil
	case "down", "j":
		a.next()
		return a, nil
	case "enter":
		return a.handleEnter()
	case "backspace":
		a.goBack()
		return a, nil
	case "tab":
		if a.showSettings && !a.settingsEditing {
			a.nextSettingsField()
		}
		return a, nil
	case "esc":
		if a.settingsEditing {
			a.cancelEditingSettings()
		}
		return a, nil
	}

	// Handle settings input
	if a.showSettings && a.settingsEditing {
		switch msg.String() {
		case "enter":
			a.saveSettings()
			return a, nil
		case "backspace":
			if len(a.settingsInput) > 0 {
				a.settingsInput = a.settingsInput[:len(a.settingsInput)-1]
			}
			return a, nil
		default:
			if len(msg.String()) == 1 {
				a.settingsInput += msg.String()
			}
			return a, nil
		}
	}

	return a, nil
}

func (a *App) handleEnter() (tea.Model, tea.Cmd) {
	switch a.state {
	case StateServerList:
		if a.selectedServer >= 0 && a.selectedServer < len(a.servers) {
			a.state = StateDirectoryBrowser
			a.currentDirectory = []string{}
			a.loadDirectory()
		}
	case StateDirectoryBrowser:
		if a.selectedItem >= 0 && a.selectedItem < len(a.directoryContents) {
			item := a.directoryContents[a.selectedItem]
			if item.IsDirectory {
				a.currentDirectory = append(a.currentDirectory, item.Name)
				a.loadDirectory()
			} else {
				// Play file
				if err := a.playSelectedFile(); err != nil {
					a.lastError = fmt.Sprintf("Failed to play file: %v", err)
				} else {
					a.lastError = ""
				}
			}
		}
	case StateFileDetails:
		a.state = StateDirectoryBrowser
	}
	return a, nil
}

func (a *App) handleDiscoveryMessage(msg DiscoveryMessage) (tea.Model, tea.Cmd) {
	switch msg.Type {
	case "started":
		a.isDiscovering = true
		a.discoveryErrors = []string{}
		log.Printf("Discovery started, servers count: %d", len(a.servers))
	case "device_found":
		if msg.Device != nil {
			// Check for duplicates
			found := false
			for _, server := range a.servers {
				if server.Location == msg.Device.Location {
					found = true
					break
				}
			}
			if !found {
				a.servers = append(a.servers, *msg.Device)
				log.Printf("Added device: %s, total servers: %d", msg.Device.Name, len(a.servers))
			} else {
				log.Printf("Duplicate device ignored: %s", msg.Device.Name)
			}
		}
	case "error":
		a.discoveryErrors = append(a.discoveryErrors, msg.Error)
		a.lastError = msg.Error
		log.Printf("Discovery error: %s", msg.Error)
	case "completed":
		a.isDiscovering = false
		log.Printf("Discovery completed, total servers: %d", len(a.servers))
		if len(a.servers) == 0 {
			a.lastError = "No UPnP devices found"
		} else {
			a.lastError = ""
		}
	}
	return a, nil
}

func (a *App) previous() {
	switch a.state {
	case StateServerList:
		if len(a.servers) > 0 {
			if a.selectedServer <= 0 {
				a.selectedServer = len(a.servers) - 1
			} else {
				a.selectedServer--
			}
		}
	case StateDirectoryBrowser:
		if len(a.directoryContents) > 0 {
			if a.selectedItem <= 0 {
				a.selectedItem = len(a.directoryContents) - 1
			} else {
				a.selectedItem--
			}
		}
	}
}

func (a *App) next() {
	switch a.state {
	case StateServerList:
		if len(a.servers) > 0 {
			if a.selectedServer >= len(a.servers)-1 {
				a.selectedServer = 0
			} else {
				a.selectedServer++
			}
		}
	case StateDirectoryBrowser:
		if len(a.directoryContents) > 0 {
			if a.selectedItem >= len(a.directoryContents)-1 {
				a.selectedItem = 0
			} else {
				a.selectedItem++
			}
		}
	}
}

func (a *App) goBack() {
	switch a.state {
	case StateDirectoryBrowser:
		if len(a.currentDirectory) == 0 {
			a.state = StateServerList
		} else {
			a.currentDirectory = a.currentDirectory[:len(a.currentDirectory)-1]
			a.loadDirectory()
		}
	case StateFileDetails:
		a.state = StateDirectoryBrowser
	}
}

func (a *App) toggleHelp() {
	a.showHelp = !a.showHelp
}

func (a *App) toggleSettings() {
	a.showSettings = !a.showSettings
	if a.showSettings {
		a.settingsEditing = false
		a.settingsField = FieldPlayer
		a.settingsInput = ""
	}
}

func (a *App) startEditingSettings() {
	a.settingsEditing = true
	switch a.settingsField {
	case FieldPlayer:
		a.settingsInput = a.config.MOP.Run
	case FieldCloseOnRun:
		if a.config.MOP.CloseOnRun {
			a.settingsInput = "true"
		} else {
			a.settingsInput = "false"
		}
	}
}

func (a *App) cancelEditingSettings() {
	a.settingsEditing = false
	a.settingsInput = ""
}

func (a *App) saveSettings() error {
	switch a.settingsField {
	case FieldPlayer:
		a.config.MOP.Run = a.settingsInput
	case FieldCloseOnRun:
		a.config.MOP.CloseOnRun = strings.ToLower(a.settingsInput) == "true" || a.settingsInput == "1"
	}
	a.settingsEditing = false
	a.settingsInput = ""
	return a.config.Save()
}

func (a *App) nextSettingsField() {
	if a.settingsField == FieldPlayer {
		a.settingsField = FieldCloseOnRun
	} else {
		a.settingsField = FieldPlayer
	}
}

func (a *App) loadDirectory() {
	if a.selectedServer < 0 || a.selectedServer >= len(a.servers) {
		return
	}

	server := a.servers[a.selectedServer]
	
	contents, err := BrowseDirectory(&server, a.currentDirectory, a.containerIDMap)
	a.directoryContents = contents
	if err != nil {
		a.lastError = err.Error()
	} else {
		a.lastError = ""
	}
	
	if len(a.directoryContents) > 0 {
		a.selectedItem = 0
	} else {
		a.selectedItem = -1
	}
}

func (a *App) playSelectedFile() error {
	if a.selectedItem < 0 || a.selectedItem >= len(a.directoryContents) {
		return fmt.Errorf("no file selected")
	}

	item := a.directoryContents[a.selectedItem]
	if item.IsDirectory {
		return fmt.Errorf("cannot play a directory")
	}

	if item.URL == "" {
		return fmt.Errorf("no URL available for this file")
	}

	return a.invokePlayer(item.URL)
}

func (a *App) invokePlayer(url string) error {
	player := a.config.MOP.Run
	closeOnRun := a.config.MOP.CloseOnRun

	if closeOnRun {
		// Run player in foreground and exit MOP
		cmd := exec.Command("sh", "-c", fmt.Sprintf("%s '%s'", player, url))
		cmd.Stdin = os.Stdin
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
		
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("failed to start %s: %v", player, err)
		}
		
		os.Exit(0)
		return nil // This line will never be reached, but satisfies the compiler
	} else {
		// Use nohup to detach player from MOP's process tree
		cmd := exec.Command("sh", "-c", fmt.Sprintf("nohup %s --really-quiet --no-terminal '%s' > /dev/null 2>&1 &", player, url))
		return cmd.Run()
	}
}

func (a *App) hasErrors() bool {
	return a.lastError != "" || len(a.discoveryErrors) > 0
}

func (a *App) copyErrorsToClipboard() {
	if len(a.discoveryErrors) == 0 {
		return
	}

	var errorsText strings.Builder
	for i, error := range a.discoveryErrors {
		errorsText.WriteString(fmt.Sprintf("%d. %s\n", i+1, error))
	}

	// Try to copy to clipboard using xclip or xsel
	cmd := exec.Command("xclip", "-selection", "clipboard")
	cmd.Stdin = strings.NewReader(errorsText.String())
	if err := cmd.Run(); err != nil {
		// Fallback to xsel
		cmd = exec.Command("xsel", "--clipboard", "--input")
		cmd.Stdin = strings.NewReader(errorsText.String())
		cmd.Run()
	}

	a.lastError = "Errors copied to clipboard"
}

func (a *App) startDiscoveryDelayed() tea.Cmd {
	return func() tea.Msg {
		// Start discovery after a short delay to ensure TUI is ready
		go a.startDiscovery()
		return nil
	}
}

func (a *App) startDiscovery() {
	a.discoveryChan <- DiscoveryMessage{Type: "started"}
	
	// Use callback-based discovery for real-time updates
	_, errors := DiscoverUpnpDevicesWithCallback(func(device UpnpDevice) {
		a.discoveryChan <- DiscoveryMessage{
			Type:   "device_found",
			Device: &device,
		}
	})
	
	// Add a small delay to ensure all device messages are processed
	time.Sleep(100 * time.Millisecond)
	
	for _, err := range errors {
		a.discoveryChan <- DiscoveryMessage{
			Type:  "error",
			Error: err,
		}
	}
	
	// Add another delay before completion
	time.Sleep(100 * time.Millisecond)
	a.discoveryChan <- DiscoveryMessage{Type: "completed"}
}

func (a *App) checkDiscoveryUpdates() tea.Cmd {
	return func() tea.Msg {
		select {
		case msg := <-a.discoveryChan:
			return msg
		default:
			return nil
		}
	}
}

type tickMsg time.Time

func (a *App) tick() tea.Cmd {
	return tea.Tick(time.Millisecond*10, func(t time.Time) tea.Msg {
		return tickMsg(t)
	})
}

func (a *App) periodicDiscovery() tea.Cmd {
	return tea.Tick(time.Second*30, func(t time.Time) tea.Msg {
		// Restart discovery every 30 seconds
		go a.startDiscovery()
		return nil
	})
}
