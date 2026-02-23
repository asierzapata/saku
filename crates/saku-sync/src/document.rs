use std::collections::BTreeMap;
use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::backend::SyncBackend;
use crate::error::SyncError;
use crate::hash::sha256_bytes;
use crate::merkle::MerkleTree;
use crate::state_db::{FileState, StateDb};

/// Same deterministic salt derivation as `sync_engine`.
fn deterministic_salt(passphrase: &[u8]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(b"saku-sync-kek-salt-v1:");
    hasher.update(passphrase);
    let hash = hasher.finalize();
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&hash[..16]);
    salt
}

/// A JSON document that can be synced independently.
///
/// Where `SyncEngine` treats an entire file (e.g. `store.json`) as one unit,
/// `DocumentSyncEngine` treats each entity — task, project, area — as its own
/// document.  This eliminates the need for schema-aware merge logic inside the
/// sync engine: every document is handled with a simple LWW comparison on its
/// `modified_at` field.
///
/// The caller is responsible for splitting the application's data into
/// documents before calling [`DocumentSyncEngine::sync`], and for
/// reassembling the returned merged documents back into the on-disk format.
#[derive(Debug, Clone)]
pub struct SyncDocument {
    /// Unique key: `"{tool}/{path}"`, e.g. `"tdo/tasks/uuid1"`.
    pub doc_key: String,
    /// Tool name, e.g. `"tdo"`.
    pub tool: String,
    /// Path within the tool, e.g. `"tasks/uuid1"`.
    /// This maps directly to the remote storage path.
    pub path: String,
    /// Plaintext JSON content of this document.
    pub content: Vec<u8>,
}

/// Configuration for [`DocumentSyncEngine`].
pub struct DocumentSyncConfig {
    /// Path to the SQLite state database.
    pub db_path: std::path::PathBuf,
    /// Passphrase for encryption/decryption.
    pub passphrase: Vec<u8>,
    /// Documents to sync in this session.
    pub documents: Vec<SyncDocument>,
}

/// Result of a document sync operation.
#[derive(Debug)]
pub enum DocumentSyncOutcome {
    /// Sync was skipped (backend unreachable, or roots matched with no local
    /// changes).
    Skipped,
    /// Sync completed successfully.
    Completed {
        /// The full reconciled document set.  The caller should use this to
        /// rebuild the authoritative on-disk store.
        merged: Vec<SyncDocument>,
        pushed: usize,
        pulled: usize,
    },
}

/// Sync engine that operates at the individual-document (entity) level.
///
/// ## How it differs from `SyncEngine`
///
/// | `SyncEngine`                        | `DocumentSyncEngine`                       |
/// |-------------------------------------|--------------------------------------------|
/// | One tracked file per tool            | One document per entity (task, project, …) |
/// | Reads/writes files on disk           | Operates on in-memory byte slices          |
/// | Schema-aware JSON merge in conflict  | Per-document LWW via `modified_at`         |
/// | Pulls merged file back to disk       | Returns merged docs; caller writes disk    |
///
/// ## Sync steps
///
/// 1. Hash each document; detect locally dirty ones.
/// 2. Fetch remote Merkle tree — fast path if roots match and nothing dirty.
/// 3. Pull remote-changed documents; apply per-document LWW.
/// 4. Push all dirty/merged documents.
/// 5. Push updated Merkle tree.
/// 6. Return the full merged document set.
pub struct DocumentSyncEngine<B: SyncBackend> {
    config: DocumentSyncConfig,
    backend: B,
    state_db: StateDb,
}

impl<B: SyncBackend> DocumentSyncEngine<B> {
    /// Create a sync engine backed by a persistent SQLite state database.
    pub fn new(config: DocumentSyncConfig, backend: B) -> Result<Self, SyncError> {
        let state_db = StateDb::open(&config.db_path)?;
        Ok(Self {
            config,
            backend,
            state_db,
        })
    }

    /// Create a sync engine backed by an in-memory state database (for tests).
    pub fn new_in_memory(config: DocumentSyncConfig, backend: B) -> Result<Self, SyncError> {
        let state_db = StateDb::open_in_memory()?;
        Ok(Self {
            config,
            backend,
            state_db,
        })
    }

    /// Run the document sync loop and return the merged document set.
    pub fn sync(&mut self) -> Result<DocumentSyncOutcome, SyncError> {
        if !self.backend.is_reachable() {
            return Ok(DocumentSyncOutcome::Skipped);
        }

        let salt = deterministic_salt(&self.config.passphrase);
        let master_key = saku_crypto::kdf::derive_master_key(&self.config.passphrase, &salt)?;

        // ── Step 1: Hash each document; detect dirty ones ───────────────────
        let mut local_docs: HashMap<String, SyncDocument> = HashMap::new();
        let mut dirty_keys: Vec<String> = Vec::new();

        for doc in &self.config.documents {
            let current_hash = sha256_bytes(&doc.content);
            let prev_state = self.state_db.get_file_state(&doc.doc_key)?;
            let is_dirty = match &prev_state {
                Some(state) => state.local_hash != current_hash,
                None => true,
            };

            if is_dirty {
                dirty_keys.push(doc.doc_key.clone());
            }

            let now_ms = jiff::Timestamp::now().as_millisecond();
            self.state_db.upsert_file_state(&FileState {
                file_key: doc.doc_key.clone(),
                local_hash: current_hash,
                remote_hash: prev_state
                    .map(|s| s.remote_hash)
                    .unwrap_or_default(),
                status: if is_dirty {
                    "dirty".to_string()
                } else {
                    "clean".to_string()
                },
                updated_at_ms: now_ms,
            })?;

            local_docs.insert(doc.doc_key.clone(), doc.clone());
        }

        // ── Step 2: Fetch remote Merkle; check fast path ─────────────────────
        let remote_merkle_data = self.backend.fetch_merkle()?;
        let remote_merkle = match &remote_merkle_data {
            Some(data) => Some(MerkleTree::from_json(data)?),
            None => None,
        };

        let mut pre_hashes: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for doc in &self.config.documents {
            let hash = sha256_bytes(&doc.content);
            pre_hashes
                .entry(doc.tool.clone())
                .or_default()
                .push((doc.path.clone(), hash));
        }
        let local_merkle = MerkleTree::build(pre_hashes);

        if let Some(ref rm) = remote_merkle {
            if local_merkle.same_root(rm) && dirty_keys.is_empty() {
                return Ok(DocumentSyncOutcome::Completed {
                    merged: self.config.documents.clone(),
                    pushed: 0,
                    pulled: 0,
                });
            }
        }

        let mut pushed = 0;
        let mut pulled = 0;

        // ── Step 3: Pull remote changes (before pushing) ─────────────────────
        if let Some(ref rm) = remote_merkle {
            let differing = local_merkle.differing_tools(rm);

            for tool_name in differing {
                if let Some(remote_tool) = rm.tools.iter().find(|t| t.tool == tool_name) {
                    for file_leaf in &remote_tool.files {
                        let doc_key = format!("{}/{}", tool_name, file_leaf.path);

                        let local_state = self.state_db.get_file_state(&doc_key)?;
                        let needs_pull = match &local_state {
                            Some(state) => state.remote_hash != file_leaf.hash,
                            None => true,
                        };

                        if !needs_pull {
                            continue;
                        }

                        let encrypted = match self.backend.fetch(tool_name, &file_leaf.path) {
                            Ok(data) => data,
                            Err(_) => continue,
                        };

                        let remote_content = saku_crypto::decrypt(&encrypted, &master_key)?;

                        // Per-document LWW: compare `modified_at`; winner keeps
                        // its entire content.  If the document only exists on
                        // the remote side, accept it unconditionally.
                        let merged_content =
                            if let Some(local_doc) = local_docs.get(&doc_key) {
                                crate::conflict::lww_merge_document(
                                    &local_doc.content,
                                    &remote_content,
                                )
                            } else {
                                remote_content
                            };

                        let merged_doc = SyncDocument {
                            doc_key: doc_key.clone(),
                            tool: tool_name.to_string(),
                            path: file_leaf.path.clone(),
                            content: merged_content,
                        };
                        local_docs.insert(doc_key.clone(), merged_doc);

                        pulled += 1;

                        let merged_hash = sha256_bytes(&local_docs[&doc_key].content);
                        let now_ms = jiff::Timestamp::now().as_millisecond();
                        self.state_db.upsert_file_state(&FileState {
                            file_key: doc_key.clone(),
                            local_hash: merged_hash,
                            remote_hash: file_leaf.hash.clone(),
                            // Mark dirty so the merged version gets pushed back.
                            status: "dirty".to_string(),
                            updated_at_ms: now_ms,
                        })?;

                        if !dirty_keys.contains(&doc_key) {
                            dirty_keys.push(doc_key);
                        }
                    }
                }
            }
        }

        // ── Step 4: Push all dirty/merged documents ──────────────────────────
        for doc_key in &dirty_keys {
            if let Some(doc) = local_docs.get(doc_key) {
                let encrypted = saku_crypto::encrypt(&doc.content, &master_key, &salt)?;
                self.backend.push(&doc.tool, &doc.path, &encrypted)?;
                pushed += 1;

                let hash = sha256_bytes(&doc.content);
                let now_ms = jiff::Timestamp::now().as_millisecond();
                self.state_db.upsert_file_state(&FileState {
                    file_key: doc_key.clone(),
                    local_hash: hash.clone(),
                    remote_hash: hash,
                    status: "clean".to_string(),
                    updated_at_ms: now_ms,
                })?;
            }
        }

        for key in &dirty_keys {
            self.state_db.clear_ops_for_file(key)?;
        }

        // ── Step 5: Push updated Merkle tree ─────────────────────────────────
        let mut updated_hashes: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for doc in local_docs.values() {
            let hash = sha256_bytes(&doc.content);
            updated_hashes
                .entry(doc.tool.clone())
                .or_default()
                .push((doc.path.clone(), hash));
        }
        let final_merkle = MerkleTree::build(updated_hashes);
        let merkle_json = final_merkle.to_json()?;
        self.backend.push_merkle(&merkle_json)?;

        let merged: Vec<SyncDocument> = local_docs.into_values().collect();
        Ok(DocumentSyncOutcome::Completed {
            merged,
            pushed,
            pulled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::local_fs::LocalFsSyncBackend;
    use serde_json::json;

    fn make_doc(id: &str, collection: &str, wall_ms: i64) -> SyncDocument {
        let path = format!("{}/{}", collection, id);
        let doc_key = format!("tdo/{}", path);
        let content = serde_json::to_vec(&json!({
            "id": id,
            "title": format!("Task {}", id),
            "modified_at": {"wall_ms": wall_ms, "lamport": 1, "device_id": "dev-a"}
        }))
        .unwrap();
        SyncDocument {
            doc_key,
            tool: "tdo".to_string(),
            path,
            content,
        }
    }

    fn make_config(docs: Vec<SyncDocument>) -> DocumentSyncConfig {
        DocumentSyncConfig {
            db_path: std::path::PathBuf::from(":memory:"),
            passphrase: b"test-passphrase".to_vec(),
            documents: docs,
        }
    }

    #[test]
    fn sync_skipped_when_unreachable() {
        let docs = vec![make_doc("uuid-1", "tasks", 1000)];
        let backend = LocalFsSyncBackend::new(std::path::Path::new("/nonexistent/remote"));
        let config = make_config(docs);
        let mut engine = DocumentSyncEngine::new_in_memory(config, backend).unwrap();

        match engine.sync().unwrap() {
            DocumentSyncOutcome::Skipped => {}
            other => panic!("Expected Skipped, got {:?}", other),
        }
    }

    #[test]
    fn basic_push_documents() {
        let remote_dir = tempfile::tempdir().unwrap();
        let docs = vec![
            make_doc("task-1", "tasks", 1000),
            make_doc("task-2", "tasks", 2000),
        ];
        let backend = LocalFsSyncBackend::new(remote_dir.path());
        let config = make_config(docs);
        let mut engine = DocumentSyncEngine::new_in_memory(config, backend).unwrap();

        match engine.sync().unwrap() {
            DocumentSyncOutcome::Completed { pushed, .. } => {
                assert_eq!(pushed, 2, "Both documents should have been pushed");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }

        // Remote should have encrypted blobs and merkle
        assert!(remote_dir.path().join("tdo/tasks/task-1.enc").exists());
        assert!(remote_dir.path().join("tdo/tasks/task-2.enc").exists());
        assert!(remote_dir.path().join("merkle.json").exists());
    }

    #[test]
    fn pull_remote_only_document() {
        let remote_dir = tempfile::tempdir().unwrap();

        // First device pushes a task
        let remote_task = make_doc("remote-task", "tasks", 1000);
        let backend1 = LocalFsSyncBackend::new(remote_dir.path());
        let mut engine1 = DocumentSyncEngine::new_in_memory(
            make_config(vec![remote_task]),
            backend1,
        )
        .unwrap();
        engine1.sync().unwrap();

        // Second device (no local tasks) syncs — should pull the remote task
        let backend2 = LocalFsSyncBackend::new(remote_dir.path());
        let mut engine2 =
            DocumentSyncEngine::new_in_memory(make_config(vec![]), backend2).unwrap();

        match engine2.sync().unwrap() {
            DocumentSyncOutcome::Completed { merged, pulled, .. } => {
                assert_eq!(pulled, 1, "Should have pulled the remote-only task");
                assert_eq!(merged.len(), 1);
                assert_eq!(merged[0].path, "tasks/remote-task");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn lww_remote_wins_when_newer() {
        let remote_dir = tempfile::tempdir().unwrap();

        // Device A pushes old version of a task
        let old_content = serde_json::to_vec(&json!({
            "id": "task-x",
            "title": "Old title",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
        }))
        .unwrap();
        let old_doc = SyncDocument {
            doc_key: "tdo/tasks/task-x".to_string(),
            tool: "tdo".to_string(),
            path: "tasks/task-x".to_string(),
            content: old_content,
        };
        let backend_a = LocalFsSyncBackend::new(remote_dir.path());
        let mut engine_a =
            DocumentSyncEngine::new_in_memory(make_config(vec![old_doc]), backend_a).unwrap();
        engine_a.sync().unwrap();

        // Device B has newer version of the same task
        let new_content = serde_json::to_vec(&json!({
            "id": "task-x",
            "title": "New title",
            "modified_at": {"wall_ms": 200, "lamport": 2, "device_id": "dev-b"}
        }))
        .unwrap();
        let new_doc = SyncDocument {
            doc_key: "tdo/tasks/task-x".to_string(),
            tool: "tdo".to_string(),
            path: "tasks/task-x".to_string(),
            content: new_content.clone(),
        };
        let backend_b = LocalFsSyncBackend::new(remote_dir.path());
        let mut engine_b =
            DocumentSyncEngine::new_in_memory(make_config(vec![new_doc]), backend_b).unwrap();
        engine_b.sync().unwrap();

        // Device A re-syncs — should pull Device B's newer version
        let stale_content = serde_json::to_vec(&json!({
            "id": "task-x",
            "title": "Old title",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
        }))
        .unwrap();
        let stale_doc = SyncDocument {
            doc_key: "tdo/tasks/task-x".to_string(),
            tool: "tdo".to_string(),
            path: "tasks/task-x".to_string(),
            content: stale_content,
        };
        let backend_a2 = LocalFsSyncBackend::new(remote_dir.path());
        let mut engine_a2 =
            DocumentSyncEngine::new_in_memory(make_config(vec![stale_doc]), backend_a2).unwrap();

        match engine_a2.sync().unwrap() {
            DocumentSyncOutcome::Completed { merged, .. } => {
                let merged_json: serde_json::Value =
                    serde_json::from_slice(&merged[0].content).unwrap();
                assert_eq!(
                    merged_json["title"], "New title",
                    "Remote (newer) version should win"
                );
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }
}
