use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mop: MopConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MopConfig {
    pub run: String,
    pub close_on_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mop: MopConfig {
                run: "mpv".to_string(),
                close_on_run: true,
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let config_path = Self::get_config_path()?;
        
        if !config_path.exists() {
            // Create default config if it doesn't exist
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }
        
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        
        toml::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))
    }
    
    pub fn save(&self) -> Result<(), String> {
        let config_path = Self::get_config_path()?;
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        fs::write(&config_path, content)
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        
        Ok(())
    }
    
    fn get_config_path() -> Result<PathBuf, String> {
        let home = std::env::var("HOME")
            .map_err(|_| "HOME environment variable not set")?;
        Ok(PathBuf::from(home).join(".config").join("mop.toml"))
    }
}
