use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub steam_api_key: String,
    pub steam_id: String,
}

/// Base data directory: %APPDATA%\Gamekeeper (Windows)
/// or ~/.gamekeeper (other platforms).
pub fn data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("Gamekeeper")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".gamekeeper")
    }
}

pub fn config_dir() -> PathBuf {
    data_dir().join("config")
}

pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

pub fn config_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn overrides_file() -> PathBuf {
    config_dir().join("overrides.json")
}

pub fn library_cache_file() -> PathBuf {
    cache_dir().join("library.json")
}

pub fn store_cache_file() -> PathBuf {
    cache_dir().join("store_details.json")
}

pub fn classifications_file() -> PathBuf {
    cache_dir().join("classifications_final.json")
}

pub fn hltb_cache_file() -> PathBuf {
    cache_dir().join("hltb_cache.json")
}

pub fn load_config() -> Result<AppConfig, String> {
    let path = config_file();
    if !path.exists() {
        return Err("No config file found. Please set up your Steam API key and Steam ID.".into());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse config: {e}"))
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;
    let data =
        serde_json::to_string_pretty(config).map_err(|e| format!("Failed to serialize: {e}"))?;
    fs::write(config_file(), data).map_err(|e| format!("Failed to write config: {e}"))
}
