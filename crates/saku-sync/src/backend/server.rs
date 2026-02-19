use crate::backend::SyncBackend;
use crate::error::SyncError;

/// Placeholder for the Phase 4 server-backed sync backend.
/// All methods currently return an error.
pub struct ServerSyncBackend;

impl SyncBackend for ServerSyncBackend {
    fn fetch(&self, _tool: &str, _path: &str) -> Result<Vec<u8>, SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend not yet implemented (Phase 4)".to_string(),
        })
    }

    fn push(&self, _tool: &str, _path: &str, _data: &[u8]) -> Result<(), SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend not yet implemented (Phase 4)".to_string(),
        })
    }

    fn fetch_merkle(&self) -> Result<Option<Vec<u8>>, SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend not yet implemented (Phase 4)".to_string(),
        })
    }

    fn push_merkle(&self, _data: &[u8]) -> Result<(), SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend not yet implemented (Phase 4)".to_string(),
        })
    }

    fn is_reachable(&self) -> bool {
        false
    }
}
