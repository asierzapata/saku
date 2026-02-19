use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Client-side sync configuration, stored at `~/.config/saku/sync.toml`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncClientConfig {
    pub server_url: String,
    pub device_id: String,
}

/// Load client sync config from `~/.config/saku/sync.toml`.
/// Returns `None` if the file doesn't exist.
pub fn load_sync_config() -> Result<Option<SyncClientConfig>, SyncConfigError> {
    let path = sync_config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(SyncConfigError::Io)?;
    let config: SyncClientConfig =
        toml::from_str(&content).map_err(|e| SyncConfigError::Parse(e.to_string()))?;
    Ok(Some(config))
}

/// Save client sync config to `~/.config/saku/sync.toml`.
pub fn save_sync_config(config: &SyncClientConfig) -> Result<(), SyncConfigError> {
    let path = sync_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SyncConfigError::Io)?;
    }
    let content =
        toml::to_string_pretty(config).map_err(|e| SyncConfigError::Parse(e.to_string()))?;
    std::fs::write(&path, content).map_err(SyncConfigError::Io)?;
    Ok(())
}

/// Delete client sync config.
pub fn delete_sync_config() -> Result<(), SyncConfigError> {
    let path = sync_config_path()?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(SyncConfigError::Io)?;
    }
    Ok(())
}

/// Returns the path to the sync config file: `~/.config/saku/sync.toml`
pub fn sync_config_path() -> Result<PathBuf, SyncConfigError> {
    dirs::config_dir()
        .map(|d| d.join("saku").join("sync.toml"))
        .ok_or(SyncConfigError::NoConfigDir)
}

#[derive(Debug, thiserror::Error)]
pub enum SyncConfigError {
    #[error("Failed to determine config directory")]
    NoConfigDir,

    #[error("I/O error: {0}")]
    Io(std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}
