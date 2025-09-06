package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	titleStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("12")).
			Bold(true)

	selectedStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("11")).
			Bold(true)

	errorStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("9"))

	infoStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("14"))

	helpStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("8"))

	modalStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("12")).
			Padding(1, 2)

	buttonStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("7")).
			Background(lipgloss.Color("0")).
			Padding(0, 1)

	activeButtonStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("0")).
				Background(lipgloss.Color("7")).
				Padding(0, 1)
)

func (a *App) View() string {
	if a.showHelp {
		return a.renderHelp()
	}

	if a.showSettings {
		return a.renderSettings()
	}

	return a.renderMain()
}

func (a *App) renderMain() string {
	var content strings.Builder

	// Title
	content.WriteString(titleStyle.Render("MOP - UPnP Device Explorer"))
	content.WriteString("\n\n")

	// Main content based on state
	switch a.state {
	case StateServerList:
		content.WriteString(a.renderServerList())
	case StateDirectoryBrowser:
		content.WriteString(a.renderDirectoryBrowser())
	case StateFileDetails:
		content.WriteString(a.renderFileDetails())
	}

	// Help text
	content.WriteString("\n")
	content.WriteString(a.renderHelpText())

	// Error display
	if a.hasErrors() {
		content.WriteString("\n")
		content.WriteString(a.renderErrorPanel())
	}

	return content.String()
}

func (a *App) renderServerList() string {
	var content strings.Builder

	// Title with discovery status
	title := "[ ] Discovered UPnP Devices"
	if a.isDiscovering {
		title = "[•] Discovered UPnP Devices"
	}
	content.WriteString(titleStyle.Render(title))
	content.WriteString("\n\n")

	// Server list
	if len(a.servers) == 0 {
		if a.isDiscovering {
			content.WriteString("Discovering devices...")
		} else {
			content.WriteString("No devices found")
		}
	} else {
		// Calculate layout dimensions
		leftWidth := a.width / 2
		
		// Left side - clean device list
		leftContent := strings.Builder{}
		for i, server := range a.servers {
			prefix := "  "
			style := lipgloss.NewStyle()
			
			if i == a.selectedServer {
				prefix = "> "
				style = selectedStyle
			}
			
			// Show only clean device name
			line := fmt.Sprintf("%s%s", prefix, server.Name)
			leftContent.WriteString(style.Render(line))
			leftContent.WriteString("\n")
		}
		
		// Right side - device details
		rightContent := strings.Builder{}
		if a.selectedServer >= 0 && a.selectedServer < len(a.servers) {
			server := a.servers[a.selectedServer]
			rightContent.WriteString(infoStyle.Render("Device Details:"))
			rightContent.WriteString("\n\n")
			rightContent.WriteString(fmt.Sprintf("Name: %s\n", server.Name))
			rightContent.WriteString(fmt.Sprintf("Type: %s\n", server.DeviceClient))
			rightContent.WriteString(fmt.Sprintf("URL: %s\n", server.Location))
			if server.BaseURL != "" {
				rightContent.WriteString(fmt.Sprintf("Base: %s\n", server.BaseURL))
			}
		}
		
		// Combine left and right sides
		leftText := leftContent.String()
		rightText := rightContent.String()
		
		// Split into lines for proper alignment
		leftLines := strings.Split(strings.TrimRight(leftText, "\n"), "\n")
		rightLines := strings.Split(strings.TrimRight(rightText, "\n"), "\n")
		
		maxLines := len(leftLines)
		if len(rightLines) > maxLines {
			maxLines = len(rightLines)
		}
		
		for i := 0; i < maxLines; i++ {
			leftLine := ""
			if i < len(leftLines) {
				leftLine = leftLines[i]
			}
			
			rightLine := ""
			if i < len(rightLines) {
				rightLine = rightLines[i]
			}
			
			// Pad left line to fixed width
			if len(leftLine) < leftWidth {
				leftLine += strings.Repeat(" ", leftWidth-len(leftLine))
			}
			
			content.WriteString(leftLine)
			content.WriteString("  ") // Separator
			content.WriteString(rightLine)
			content.WriteString("\n")
		}
	}

	return content.String()
}

func (a *App) renderDirectoryBrowser() string {
	var content strings.Builder

	// Current path
	path := "/"
	if len(a.currentDirectory) > 0 {
		path = "/" + strings.Join(a.currentDirectory, "/")
	}
	content.WriteString(titleStyle.Render(fmt.Sprintf("Directory: %s", path)))
	content.WriteString("\n\n")

	// Directory contents
	if len(a.directoryContents) == 0 {
		content.WriteString("Empty directory")
	} else {
		for i, item := range a.directoryContents {
			prefix := "  "
			style := lipgloss.NewStyle()
			
			if i == a.selectedItem {
				prefix = "> "
				style = selectedStyle
			}
			
			icon := "📄"
			if item.IsDirectory {
				icon = "📁"
			}
			
			line := fmt.Sprintf("%s%s %s", prefix, icon, item.Name)
			content.WriteString(style.Render(line))
			content.WriteString("\n")
		}
	}

	// File info panel
	if a.selectedItem >= 0 && a.selectedItem < len(a.directoryContents) {
		content.WriteString("\n")
		content.WriteString(a.renderFileInfo())
	}

	return content.String()
}

func (a *App) renderFileDetails() string {
	var content strings.Builder

	if a.selectedItem >= 0 && a.selectedItem < len(a.directoryContents) {
		item := a.directoryContents[a.selectedItem]
		
		content.WriteString(titleStyle.Render("File Details"))
		content.WriteString("\n\n")
		
		content.WriteString(fmt.Sprintf("File: %s\n", item.Name))
		
		if item.URL != "" {
			content.WriteString(fmt.Sprintf("Direct URL: %s\n", item.URL))
		}
		
		if item.Metadata != nil {
			if item.Metadata.Size != nil {
				content.WriteString(fmt.Sprintf("Size: %s\n", formatSize(*item.Metadata.Size)))
			}
			if item.Metadata.Duration != nil {
				content.WriteString(fmt.Sprintf("Duration: %s\n", *item.Metadata.Duration))
			}
			if item.Metadata.Format != nil {
				content.WriteString(fmt.Sprintf("Format: %s\n", *item.Metadata.Format))
			}
		}
	}

	return content.String()
}

func (a *App) renderFileInfo() string {
	var content strings.Builder

	if a.selectedItem >= 0 && a.selectedItem < len(a.directoryContents) {
		item := a.directoryContents[a.selectedItem]
		
		content.WriteString(infoStyle.Render("File Info:"))
		content.WriteString("\n")
		content.WriteString(fmt.Sprintf("Name: %s\n", item.Name))
		content.WriteString(fmt.Sprintf("Type: %s\n", 
			map[bool]string{true: "Directory", false: "File"}[item.IsDirectory]))
		
		if item.URL != "" {
			content.WriteString(fmt.Sprintf("URL: %s\n", item.URL))
		}
		
		if item.Metadata != nil {
			if item.Metadata.Size != nil {
				content.WriteString(fmt.Sprintf("Size: %s\n", formatSize(*item.Metadata.Size)))
			}
			if item.Metadata.Duration != nil {
				content.WriteString(fmt.Sprintf("Duration: %s\n", *item.Metadata.Duration))
			}
			if item.Metadata.Format != nil {
				content.WriteString(fmt.Sprintf("Format: %s\n", *item.Metadata.Format))
			}
		}
	}

	return content.String()
}

func (a *App) renderHelpText() string {
	var helpText string

	switch a.state {
	case StateServerList:
		if a.hasErrors() {
			helpText = fmt.Sprintf("─────| %s |─────| %s |─────| %s |─────| %s |─────| %s |─────| %s |─────",
				Keys.Navigate, Keys.SelectServer, ErrorKey, Keys.Help, Keys.Settings, Keys.Quit)
		} else {
			helpText = fmt.Sprintf("─────| %s |─────| %s |─────| %s |─────| %s |─────| %s |─────",
				Keys.Navigate, Keys.SelectServer, Keys.Help, Keys.Settings, Keys.Quit)
		}
	case StateDirectoryBrowser:
		helpText = fmt.Sprintf("─────| %s |─────| %s |─────| %s |─────| %s |─────| %s |─────| %s |─────",
			Keys.Navigate, Keys.Open, Keys.Back, Keys.Help, Keys.Settings, Keys.Quit)
	case StateFileDetails:
		helpText = fmt.Sprintf("─────| %s |─────| %s |─────| %s |─────| %s |─────",
			Keys.BackToDirectory, Keys.Help, Keys.Settings, Keys.Quit)
	}

	return helpStyle.Render(helpText)
}

func (a *App) renderErrorPanel() string {
	var content strings.Builder

	content.WriteString(errorStyle.Render("Errors:"))
	content.WriteString("\n")

	if a.lastError != "" {
		content.WriteString(fmt.Sprintf("• %s\n", a.lastError))
	}

	for i, err := range a.discoveryErrors {
		content.WriteString(fmt.Sprintf("%d. %s\n", i+1, err))
	}

	if len(a.discoveryErrors) > 0 {
		content.WriteString("\nPress 'e' to copy errors to clipboard")
	}

	return content.String()
}

func (a *App) renderHelp() string {
	helpText := `
MOP - UPnP Device Explorer

Vibecoded for Omarchy: discover UPnP devices and
browse media content directly. Press Enter on
files to play them with mpv.

Keys:
` + Keys.Navigate + `
` + Keys.SelectServer + `
` + Keys.Open + `
` + Keys.Back + `
` + Keys.Help + `
` + Keys.Settings + `
` + Keys.Quit + `

Press ? to close
`

	return modalStyle.Render(helpText)
}

func (a *App) renderSettings() string {
	var content strings.Builder

	content.WriteString(titleStyle.Render("Settings"))
	content.WriteString("\n\n")

	if a.settingsEditing {
		// Show input field
		fieldName := "Player"
		if a.settingsField == FieldCloseOnRun {
			fieldName = "Close on run"
		}
		
		content.WriteString(fmt.Sprintf("%s: %s_", fieldName, a.settingsInput))
		content.WriteString("\n\n")
		content.WriteString("Press Enter to save, Esc to cancel")
	} else {
		// Show settings overview
		playerStyle := lipgloss.NewStyle()
		closeStyle := lipgloss.NewStyle()
		
		if a.settingsField == FieldPlayer {
			playerStyle = selectedStyle
		} else {
			closeStyle = selectedStyle
		}
		
		content.WriteString(fmt.Sprintf("%s: %s\n", 
			playerStyle.Render("Player"), a.config.MOP.Run))
		
		closeValue := "No"
		if a.config.MOP.CloseOnRun {
			closeValue = "Yes"
		}
		content.WriteString(fmt.Sprintf("%s: %s\n", 
			closeStyle.Render("Close on run"), closeValue))
		
		content.WriteString("\n")
		content.WriteString("Config file: ~/.config/mop.toml\n")
		content.WriteString("\n")
		content.WriteString("Navigation: e: edit, Tab: next field, ,: close")
	}

	return modalStyle.Render(content.String())
}

func formatSize(bytes uint64) string {
	units := []string{"B", "KB", "MB", "GB", "TB"}
	size := float64(bytes)
	unitIndex := 0

	for size >= 1024.0 && unitIndex < len(units)-1 {
		size /= 1024.0
		unitIndex++
	}

	return fmt.Sprintf("%.2f %s", size, units[unitIndex])
}
