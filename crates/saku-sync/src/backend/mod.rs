pub mod local_fs;
pub mod server;

#[cfg(feature = "server")]
pub mod server_types;

use crate::error::SyncError;

/// Trait abstracting the remote storage backend.
///
/// Phase 3 uses `LocalFsSyncBackend` (a directory on disk).
/// Phase 4 will add `ServerSyncBackend` (ureq + JWT).
///
/// Supports both file-based sync (for backwards compatibility) and
/// document-based sync (for platforms without filesystem access, like iOS).
pub trait SyncBackend {
    /// Fetch a file's encrypted bytes from the remote.
    fn fetch(&self, tool: &str, path: &str) -> Result<Vec<u8>, SyncError>;

    /// Push encrypted bytes to the remote.
    fn push(&self, tool: &str, path: &str, data: &[u8]) -> Result<(), SyncError>;

    /// Fetch the remote Merkle tree JSON, or `None` if it doesn't exist yet.
    fn fetch_merkle(&self) -> Result<Option<Vec<u8>>, SyncError>;

    /// Push the Merkle tree JSON to the remote.
    fn push_merkle(&self, data: &[u8]) -> Result<(), SyncError>;

    /// Check whether the backend is reachable.
    fn is_reachable(&self) -> bool;

    // --- Document-based sync operations ---

    /// Fetch a JSON document by key. Returns encrypted JSON bytes.
    /// Document keys have format: `{tool}/{document_id}`
    fn fetch_document(&self, tool: &str, document_id: &str) -> Result<Vec<u8>, SyncError> {
        // Default implementation delegates to file-based fetch for backwards compatibility
        self.fetch(tool, document_id)
    }

    /// Push a JSON document by key. Accepts encrypted JSON bytes.
    fn push_document(&self, tool: &str, document_id: &str, data: &[u8]) -> Result<(), SyncError> {
        // Default implementation delegates to file-based push for backwards compatibility
        self.push(tool, document_id, data)
    }

    /// List all document keys for a given tool.
    /// Returns a list of document IDs (without the tool prefix).
    fn list_documents(&self, tool: &str) -> Result<Vec<String>, SyncError>;
}
