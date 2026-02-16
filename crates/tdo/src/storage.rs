use std::path::PathBuf;

use thiserror::Error;

use crate::models::store::Store;

pub mod json;
pub mod migrations;

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
        "Store file was created by a newer version of tdo (version {0}). Please upgrade tdo to open this file."
    )]
    FutureVersion(u32),

    #[error("Store file has unsupported version {0}. This version of tdo cannot read this file.")]
    UnsupportedVersion(u32),
}

pub trait Storage {
    fn load(&self) -> Result<Store, StorageError>;
    fn save(&self, store: &Store) -> Result<(), StorageError>;
}
