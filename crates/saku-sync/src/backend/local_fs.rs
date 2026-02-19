use std::path::{Path, PathBuf};

use crate::backend::SyncBackend;
use crate::error::SyncError;

/// A sync backend that stores files in a local directory tree.
/// Used for Phase 3 testing. Layout:
///   `{root}/{tool}/{path}.enc`
///   `{root}/merkle.json`
pub struct LocalFsSyncBackend {
    root: PathBuf,
}

impl LocalFsSyncBackend {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn file_path(&self, tool: &str, path: &str) -> PathBuf {
        self.root.join(tool).join(format!("{}.enc", path))
    }

    fn merkle_path(&self) -> PathBuf {
        self.root.join("merkle.json")
    }
}

impl SyncBackend for LocalFsSyncBackend {
    fn fetch(&self, tool: &str, path: &str) -> Result<Vec<u8>, SyncError> {
        let file_path = self.file_path(tool, path);
        std::fs::read(&file_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SyncError::Backend {
                    message: format!("File not found: {}/{}", tool, path),
                }
            } else {
                SyncError::Io(e)
            }
        })
    }

    fn push(&self, tool: &str, path: &str, data: &[u8]) -> Result<(), SyncError> {
        let file_path = self.file_path(tool, path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, data)?;
        Ok(())
    }

    fn fetch_merkle(&self) -> Result<Option<Vec<u8>>, SyncError> {
        let path = self.merkle_path();
        match std::fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SyncError::Io(e)),
        }
    }

    fn push_merkle(&self, data: &[u8]) -> Result<(), SyncError> {
        let path = self.merkle_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(())
    }

    fn is_reachable(&self) -> bool {
        self.root.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_fetch_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());

        let data = b"encrypted content";
        backend.push("tdo", "store.json", data).unwrap();

        let fetched = backend.fetch("tdo", "store.json").unwrap();
        assert_eq!(fetched, data);
    }

    #[test]
    fn fetch_missing_file_error() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());

        let result = backend.fetch("tdo", "nonexistent.json");
        assert!(result.is_err());
    }

    #[test]
    fn merkle_operations() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());

        // No merkle initially
        assert!(backend.fetch_merkle().unwrap().is_none());

        // Push merkle
        let data = b"{\"root\":\"abc\"}";
        backend.push_merkle(data).unwrap();

        // Fetch merkle
        let fetched = backend.fetch_merkle().unwrap().unwrap();
        assert_eq!(fetched, data);
    }

    #[test]
    fn is_reachable() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        assert!(backend.is_reachable());

        let unreachable = LocalFsSyncBackend::new(Path::new("/nonexistent/path"));
        assert!(!unreachable.is_reachable());
    }
}
