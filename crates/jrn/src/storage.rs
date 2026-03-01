use std::path::PathBuf;

use thiserror::Error;

use crate::models::store::Store;

pub mod json;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Failed to load store from '{path}': {source}")]
    LoadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse JSON from '{path}': {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to save store to '{path}': {source}")]
    SaveFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to serialize store to JSON: {source}")]
    SerializeFailed {
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to create backup at '{path}': {source}")]
    BackupFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to cleanup old backups in '{dir}': {source}")]
    CleanupFailed {
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Store file was created by a newer version of jrn (version {0}). Please upgrade jrn."
    )]
    FutureVersion(u32),
}

pub trait Storage {
    fn load(&self) -> Result<Store, StorageError>;
    fn save(&self, store: &Store) -> Result<(), StorageError>;
}

/// Returns the default path to the jrn store file.
///
/// Respects the `JRN_DATA_DIR` environment variable. Falls back to
/// `<local-data-dir>/jrn/store.json` (e.g. `~/.local/share/jrn/store.json` on Linux).
pub fn default_storage_path() -> PathBuf {
    std::env::var_os("JRN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("jrn")
        })
        .join("store.json")
}
