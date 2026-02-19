use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("Failed to write to '{path}': {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to rename '{from}' to '{to}': {source}")]
    RenameFailed {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to acquire lock on '{path}': {source}")]
    LockFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to release lock on '{path}': {source}")]
    UnlockFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
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
}
