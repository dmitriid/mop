package main


// AppState represents the current state of the application
type AppState int

const (
	StateServerList AppState = iota
	StateDirectoryBrowser
	StateFileDetails
)

// SettingsField represents which field is being edited in settings
type SettingsField int

const (
	FieldPlayer SettingsField = iota
	FieldCloseOnRun
)

// UpnpDevice represents a discovered UPnP device
type UpnpDevice struct {
	Name                 string
	Location             string
	BaseURL              string
	DeviceClient         string
	ContentDirectoryURL  string
}

// DirectoryItem represents an item in a directory listing
type DirectoryItem struct {
	Name       string
	IsDirectory bool
	URL        string
	Metadata   *FileMetadata
}

// FileMetadata contains file information
type FileMetadata struct {
	Size     *uint64
	Duration *string
	Format   *string
}

// DiscoveryMessage represents messages from the discovery process
type DiscoveryMessage struct {
	Type      string
	Device    *UpnpDevice
	Error     string
	Completed bool
	Devices   []UpnpDevice
}

// App represents the main application state
type App struct {
	state                AppState
	servers              []UpnpDevice
	selectedServer       int
	currentDirectory     []string
	directoryContents    []DirectoryItem
	selectedItem         int
	statusMessage        string
	lastError            string
	discoveryErrors      []string
	isDiscovering        bool
	showHelp             bool
	showSettings         bool
	settingsEditing      bool
	settingsField        SettingsField
	settingsInput        string
	containerIDMap       map[string]string // path -> container ID mapping
	config               *Config
	discoveryChan        chan DiscoveryMessage
	width                int
	height               int
}

// KeyMappings contains the help text for key bindings
type KeyMappings struct {
	Navigate        string
	SelectServer    string
	Open            string
	Back            string
	BackToDirectory string
	Help            string
	Settings        string
	Quit            string
}

var Keys = KeyMappings{
	Navigate:        "↑↓: navigate",
	SelectServer:    "enter: select server",
	Open:            "enter: play/open",
	Back:            "backspace: back",
	BackToDirectory: "enter: back to directory",
	Help:            "?: help",
	Settings:        ",: settings",
	Quit:            "q: quit",
}

const ErrorKey = "e: dump errors"
