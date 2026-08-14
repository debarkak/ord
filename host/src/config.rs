use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub display: DisplaySettings,
    pub stream: StreamSettings,
    pub security: SecuritySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub enable_discovery: bool,
    pub discovery_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub default_width: u32,
    pub default_height: u32,
    pub default_fps: u32,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSettings {
    pub codec: String,
    pub bitrate_kbps: u32,
    pub encoder: String, // "auto", "vaapi", "software"
    pub keyframe_interval_frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub require_pairing: bool,
    pub pairing_pin: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 9090,
                enable_discovery: true,
                discovery_port: 9091,
            },
            display: DisplaySettings {
                default_width: 1920,
                default_height: 1200,
                default_fps: 60,
                scale_factor: 1.0,
            },
            stream: StreamSettings {
                codec: "h264".to_string(),
                bitrate_kbps: 15000,
                encoder: "auto".to_string(),
                keyframe_interval_frames: 60,
            },
            security: SecuritySettings {
                require_pairing: false,
                pairing_pin: None,
            },
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ord")
            .join("config.toml")
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
