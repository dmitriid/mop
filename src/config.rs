use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mop: MopConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MopConfig {
    pub run: String,
    pub auto_close: bool,
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
            run: "mpv".to_string(),
            auto_close: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = get_config_path();
        
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    // Parse as simple TOML manually since we don't have the toml crate
                    Self::parse_toml(&content).unwrap_or_else(|_| {
                        eprintln!("Warning: Invalid config file, using defaults");
                        Self::default()
                    })
                }
                Err(_) => Self::default()
            }
        } else {
            // Create default config file
            let default_config = Self::default();
            let _ = std::fs::create_dir_all(config_path.parent().unwrap());
            let _ = std::fs::write(&config_path, default_config.to_toml());
            default_config
        }
    }
    
    pub fn save(&self) -> Result<(), String> {
        let config_path = get_config_path();
        
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        std::fs::write(&config_path, self.to_toml())
            .map_err(|e| format!("Failed to write config file: {}", e))?;
            
        Ok(())
    }
    
    fn parse_toml(content: &str) -> Result<Self, String> {
        let mut run = "mpv".to_string();
        let mut auto_close = true;
        
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("run = ") {
                if let Some(value) = line.strip_prefix("run = ") {
                    run = value.trim_matches('"').to_string();
                }
            } else if line.starts_with("auto_close = ") {
                if let Some(value) = line.strip_prefix("auto_close = ") {
                    auto_close = value.trim() == "true";
                }
            }
        }
        
        Ok(Config {
            mop: MopConfig { run, auto_close },
        })
    }
    
    fn to_toml(&self) -> String {
        format!(
            "[mop]\nrun = \"{}\"\nauto_close = {}\n",
            self.mop.run, self.mop.auto_close
        )
    }
}

fn get_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("mop.toml")
    } else {
        PathBuf::from("mop.toml") // Fallback to current directory
    }
}