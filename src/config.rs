use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mop: MopConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MopConfig {
    #[serde(default = "default_run")]
    pub run: String,
    #[serde(default)]
    pub auto_close: bool,
}

fn default_run() -> String {
    "mpv".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mop: MopConfig::default(),
        }
    }
}

impl Default for MopConfig {
    fn default() -> Self {
        Self {
            run: default_run(),
            auto_close: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = get_config_path();

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    toml::from_str(&content).unwrap_or_else(|e| {
                        eprintln!("Warning: Invalid config file: {}, using defaults", e);
                        Self::default()
                    })
                }
                Err(_) => Self::default(),
            }
        } else {
            // Create default config file
            let default_config = Self::default();
            let _ = std::fs::create_dir_all(config_path.parent().unwrap());
            if let Ok(toml_str) = toml::to_string_pretty(&default_config) {
                let _ = std::fs::write(&config_path, toml_str);
            }
            default_config
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path = get_config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        std::fs::write(&config_path, toml_str)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        Ok(())
    }
}

fn get_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("mop.toml")
    } else {
        PathBuf::from("mop.toml")
    }
}
