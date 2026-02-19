pub mod local_fs;
pub mod server;

use crate::error::SyncError;

/// Trait abstracting the remote storage backend.
///
/// Phase 3 uses `LocalFsSyncBackend` (a directory on disk).
/// Phase 4 will add `ServerSyncBackend` (ureq + JWT).
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
}
