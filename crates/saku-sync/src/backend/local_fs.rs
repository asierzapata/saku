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

    fn list_documents(&self, tool: &str) -> Result<Vec<String>, SyncError> {
        let tool_dir = self.root.join(tool);
        if !tool_dir.exists() {
            return Ok(Vec::new());
        }

        let mut documents = Vec::new();
        for entry in std::fs::read_dir(&tool_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Remove .enc suffix if present
                    let doc_id = if let Some(stripped) = file_name.strip_suffix(".enc") {
                        stripped.to_string()
                    } else {
                        file_name.to_string()
                    };
                    documents.push(doc_id);
                }
            }
        }
        Ok(documents)
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

    #[test]
    fn list_documents_empty() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        
        let docs = backend.list_documents("tdo").unwrap();
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn list_documents_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        
        // Push some documents
        backend.push("tdo", "store.json", b"data1").unwrap();
        backend.push("tdo", "config.json", b"data2").unwrap();
        backend.push("nte", "note.md", b"data3").unwrap();
        
        let tdo_docs = backend.list_documents("tdo").unwrap();
        assert_eq!(tdo_docs.len(), 2);
        assert!(tdo_docs.contains(&"store.json".to_string()));
        assert!(tdo_docs.contains(&"config.json".to_string()));
        
        let nte_docs = backend.list_documents("nte").unwrap();
        assert_eq!(nte_docs.len(), 1);
        assert!(nte_docs.contains(&"note.md".to_string()));
    }
}
