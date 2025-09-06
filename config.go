package main

import (
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

type Config struct {
	MOP MOPConfig `toml:"mop"`
}

type MOPConfig struct {
	Run        string `toml:"run"`
	CloseOnRun bool   `toml:"close_on_run"`
}

func DefaultConfig() *Config {
	return &Config{
		MOP: MOPConfig{
			Run:        "mpv",
			CloseOnRun: true,
		},
	}
}

func LoadConfig() (*Config, error) {
	configPath, err := getConfigPath()
	if err != nil {
		return nil, err
	}

	if _, err := os.Stat(configPath); os.IsNotExist(err) {
		// Create default config if it doesn't exist
		config := DefaultConfig()
		if err := config.Save(); err != nil {
			return nil, err
		}
		return config, nil
	}

	content, err := os.ReadFile(configPath)
	if err != nil {
		return nil, err
	}

	var config Config
	if err := toml.Unmarshal(content, &config); err != nil {
		return nil, err
	}

	return &config, nil
}

func (c *Config) Save() error {
	configPath, err := getConfigPath()
	if err != nil {
		return err
	}

	// Create config directory if it doesn't exist
	if err := os.MkdirAll(filepath.Dir(configPath), 0755); err != nil {
		return err
	}

	file, err := os.Create(configPath)
	if err != nil {
		return err
	}
	defer file.Close()

	encoder := toml.NewEncoder(file)
	return encoder.Encode(c)
}

func getConfigPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".config", "mop.toml"), nil
}
