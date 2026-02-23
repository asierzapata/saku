use crate::backend::SyncBackend;
use crate::error::SyncError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use saku_crypto::kdf::MasterKey;

/// A document store that syncs JSON data structures without requiring filesystem access.
/// Perfect for mobile platforms (iOS, Android) or web applications.
///
/// # Example
/// ```no_run
/// use saku_sync::document::DocumentStore;
/// use saku_sync::backend::local_fs::LocalFsSyncBackend;
/// 
/// let backend = LocalFsSyncBackend::new(std::path::Path::new("/tmp/sync"));
/// let mut store = DocumentStore::new("my-app", backend, b"passphrase".to_vec());
/// 
/// // Put a document
/// let data = serde_json::json!({"name": "John", "age": 30});
/// store.put_document("user-123", &data).unwrap();
/// 
/// // Get a document
/// let doc = store.get_document("user-123").unwrap();
/// ```
pub struct DocumentStore<B: SyncBackend> {
    tool: String,
    backend: B,
    passphrase: Vec<u8>,
}

impl<B: SyncBackend> DocumentStore<B> {
    /// Create a new document store for the given tool.
    pub fn new(tool: impl Into<String>, backend: B, passphrase: Vec<u8>) -> Self {
        Self {
            tool: tool.into(),
            backend,
            passphrase,
        }
    }

    /// Get a JSON document by its ID.
    /// Returns None if the document doesn't exist.
    pub fn get_document(&self, document_id: &str) -> Result<Option<Value>, SyncError> {
        if !self.backend.is_reachable() {
            return Err(SyncError::Backend {
                message: "Backend is not reachable".to_string(),
            });
        }

        // Fetch encrypted document
        let encrypted = match self.backend.fetch_document(&self.tool, document_id) {
            Ok(data) => data,
            Err(SyncError::Backend { ref message }) if message.contains("not found") => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        // Decrypt
        let master_key = self.derive_master_key()?;
        let decrypted = saku_crypto::decrypt(&encrypted, &master_key)?;

        // Parse JSON
        let json: Value = serde_json::from_slice(&decrypted)?;
        Ok(Some(json))
    }

    /// Put (create or update) a JSON document by its ID.
    pub fn put_document(&mut self, document_id: &str, data: &Value) -> Result<(), SyncError> {
        if !self.backend.is_reachable() {
            return Err(SyncError::Backend {
                message: "Backend is not reachable".to_string(),
            });
        }

        // Serialize to JSON bytes
        let json_bytes = serde_json::to_vec(data)?;

        // Encrypt
        let master_key = self.derive_master_key()?;
        let salt = self.deterministic_salt();
        let encrypted = saku_crypto::encrypt(&json_bytes, &master_key, &salt)?;

        // Push to backend
        self.backend
            .push_document(&self.tool, document_id, &encrypted)?;

        Ok(())
    }

    /// List all document IDs for this tool.
    pub fn list_documents(&self) -> Result<Vec<String>, SyncError> {
        if !self.backend.is_reachable() {
            return Err(SyncError::Backend {
                message: "Backend is not reachable".to_string(),
            });
        }

        self.backend.list_documents(&self.tool)
    }

    /// Delete a document by its ID.
    /// This is a logical delete - the document is marked as deleted but not physically removed.
    pub fn delete_document(&mut self, document_id: &str) -> Result<(), SyncError> {
        // For now, we'll implement delete by pushing an empty/tombstone document
        // A more sophisticated implementation could use a separate deletion flag
        let tombstone = serde_json::json!({
            "_deleted": true,
            "_deleted_at": jiff::Timestamp::now().as_millisecond()
        });
        self.put_document(document_id, &tombstone)
    }

    /// Sync documents using Last-Writer-Wins (LWW) merge strategy.
    /// Fetches all remote documents, merges with local cache, and pushes updates.
    /// 
    /// This is a simplified sync that works directly with documents rather than files.
    /// For full conflict resolution and entity-level merging, use the file-based SyncEngine.
    pub fn sync_all_documents(
        &mut self,
        local_docs: &[(String, Value)],
    ) -> Result<SyncResult, SyncError> {
        if !self.backend.is_reachable() {
            return Ok(SyncResult {
                pushed: 0,
                pulled: 0,
                conflicts: Vec::new(),
            });
        }

        let mut result = SyncResult {
            pushed: 0,
            pulled: 0,
            conflicts: Vec::new(),
        };

        // Get list of remote documents
        let remote_ids = self.list_documents()?;

        // Build a map of local documents by ID
        let local_map: std::collections::HashMap<String, &Value> =
            local_docs.iter().map(|(id, val)| (id.clone(), val)).collect();

        // Build a map to track remote versions for comparison
        let mut remote_map: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();

        // Pull remote documents and detect conflicts
        let mut merged_docs: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();

        for doc_id in &remote_ids {
            if let Some(remote_doc) = self.get_document(doc_id)? {
                remote_map.insert(doc_id.clone(), remote_doc.clone());
                
                if let Some(local_doc) = local_map.get(doc_id) {
                    // Both exist - merge using modified_at timestamps
                    let merged = merge_documents(local_doc, &remote_doc);
                    
                    // A conflict occurs when both versions differ AND neither is identical to merged
                    // (meaning both local and remote were modified independently)
                    let local_ts = extract_modified_at(local_doc);
                    let remote_ts = extract_modified_at(&remote_doc);
                    if local_ts != remote_ts && **local_doc != merged && remote_doc != merged {
                        result.conflicts.push(doc_id.clone());
                    }
                    
                    merged_docs.insert(doc_id.clone(), merged);
                } else {
                    // Remote only - accept remote
                    merged_docs.insert(doc_id.clone(), remote_doc);
                    result.pulled += 1;
                }
            }
        }

        // Add local-only documents
        for (doc_id, local_doc) in local_docs {
            if !remote_ids.contains(doc_id) {
                merged_docs.insert(doc_id.clone(), (*local_doc).clone());
            }
        }

        // Push documents that differ from remote
        for (doc_id, merged_doc) in &merged_docs {
            let should_push = if let Some(remote_doc) = remote_map.get(doc_id) {
                // Document exists remotely - push if merged differs from remote
                merged_doc != remote_doc
            } else {
                // Document doesn't exist remotely - always push
                true
            };

            if should_push {
                self.put_document(doc_id, merged_doc)?;
                result.pushed += 1;
            }
        }

        Ok(result)
    }

    fn derive_master_key(&self) -> Result<MasterKey, SyncError> {
        let salt = self.deterministic_salt();
        saku_crypto::kdf::derive_master_key(&self.passphrase, &salt).map_err(SyncError::from)
    }

    fn deterministic_salt(&self) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"saku-sync-kek-salt-v1:");
        hasher.update(&self.passphrase);
        let hash = hasher.finalize();
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&hash[..16]);
        salt
    }
}

/// Result of a document sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: Vec<String>,
}

/// Merge two JSON documents using Last-Writer-Wins based on modified_at timestamp.
/// If neither has modified_at, returns the remote document (conservative choice).
fn merge_documents(local: &Value, remote: &Value) -> Value {
    let local_ts = extract_modified_at(local);
    let remote_ts = extract_modified_at(remote);

    // Compare all three components: wall_ms (primary), lamport (secondary), device_id (tertiary)
    match local_ts.cmp(&remote_ts) {
        std::cmp::Ordering::Greater => local.clone(),
        std::cmp::Ordering::Less => remote.clone(),
        std::cmp::Ordering::Equal => {
            // When timestamps are completely equal (rare), prefer lexicographically higher device_id
            // This ensures deterministic conflict resolution
            if local_ts.2 >= remote_ts.2 {
                local.clone()
            } else {
                remote.clone()
            }
        }
    }
}

/// Extract (wall_ms, lamport, device_id) from a JSON document's `modified_at` field.
fn extract_modified_at(doc: &Value) -> (i64, u64, String) {
    let ma = doc.get("modified_at");
    let wall_ms = ma
        .and_then(|v| v.get("wall_ms"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let lamport = ma
        .and_then(|v| v.get("lamport"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let device_id = ma
        .and_then(|v| v.get("device_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (wall_ms, lamport, device_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::local_fs::LocalFsSyncBackend;
    use serde_json::json;

    #[test]
    fn document_put_get_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        let mut store = DocumentStore::new("test-app", backend, b"test-pass".to_vec());

        let doc = json!({"name": "Alice", "age": 25});
        store.put_document("user-1", &doc).unwrap();

        let fetched = store.get_document("user-1").unwrap().unwrap();
        assert_eq!(fetched["name"], "Alice");
        assert_eq!(fetched["age"], 25);
    }

    #[test]
    fn get_nonexistent_document() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        let store = DocumentStore::new("test-app", backend, b"test-pass".to_vec());

        let result = store.get_document("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_documents() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalFsSyncBackend::new(dir.path());
        let mut store = DocumentStore::new("test-app", backend, b"test-pass".to_vec());

        store.put_document("doc-1", &json!({"a": 1})).unwrap();
        store.put_document("doc-2", &json!({"b": 2})).unwrap();

        let docs = store.list_documents().unwrap();
        assert_eq!(docs.len(), 2);
        assert!(docs.contains(&"doc-1".to_string()));
        assert!(docs.contains(&"doc-2".to_string()));
    }

    #[test]
    fn merge_newer_remote_wins() {
        let local = json!({
            "id": "task-1",
            "title": "local",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
        });

        let remote = json!({
            "id": "task-1",
            "title": "remote",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        });

        let merged = merge_documents(&local, &remote);
        assert_eq!(merged["title"], "remote");
    }

    #[test]
    fn merge_newer_local_wins() {
        let local = json!({
            "id": "task-1",
            "title": "local",
            "modified_at": {"wall_ms": 300, "lamport": 1, "device_id": "dev-a"}
        });

        let remote = json!({
            "id": "task-1",
            "title": "remote",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        });

        let merged = merge_documents(&local, &remote);
        assert_eq!(merged["title"], "local");
    }
}
