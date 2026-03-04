use std::cell::RefCell;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use saku_crypto::kdf::{derive_deterministic_salt, derive_master_key};
use saku_crypto::{decrypt_entry, encrypt_entry};
use saku_storage::dirty_tracker::DirtyTracker;
use saku_storage::kv_store::{self, KvStore};

use crate::conflict::{fix_duplicate_task_numbers_kv, tdo_entity_schemas};
use crate::error::SyncError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Outcome of a KV sync operation.
#[derive(Debug)]
pub struct KvSyncOutcome {
    pub pulled: usize,
    pub pushed: usize,
}

/// Configuration for a KV sync run.
pub struct KvSyncConfig {
    pub server_url: String,
    pub tool: String,
    pub passphrase: Vec<u8>,
    pub store_path: PathBuf,
}

// ---------------------------------------------------------------------------
// HTTP response/request types (match server API)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GetEntriesResponse {
    entries: Vec<KvEntryResponse>,
    cookie: String,
    has_more: bool,
}

#[derive(Deserialize)]
struct KvEntryResponse {
    key: String,
    blob: String, // base64
    #[allow(dead_code)]
    seq: i64,
    #[allow(dead_code)]
    deleted: bool,
}

#[derive(Serialize)]
struct BatchPutRequest {
    entries: Vec<BatchPutEntry>,
}

#[derive(Serialize, Clone)]
struct BatchPutEntry {
    key: String,
    blob: String, // base64
}

#[derive(Deserialize)]
struct BatchPutResponse {
    #[allow(dead_code)]
    results: Vec<BatchPutResult>,
    cookie: String,
}

#[derive(Deserialize)]
struct BatchPutResult {
    #[allow(dead_code)]
    key: String,
    #[allow(dead_code)]
    seq: i64,
}

// ---------------------------------------------------------------------------
// Auth refresh types (same as backend/server.rs)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

// ---------------------------------------------------------------------------
// HTTP client with auth retry
// ---------------------------------------------------------------------------

struct KvHttpClient {
    server_url: String,
    access_token: RefCell<String>,
    refresh_token: RefCell<String>,
    http: ureq::Agent,
}

impl KvHttpClient {
    fn new(server_url: &str) -> Result<Self, SyncError> {
        let creds = saku_crypto::keychain::SyncCredentialStore::new()
            .and_then(|s| s.load_or_migrate())
            .unwrap_or_default();

        let http = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(5))
            .timeout_read(std::time::Duration::from_secs(30))
            .build();

        Ok(Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            access_token: RefCell::new(creds.access_token.unwrap_or_default()),
            refresh_token: RefCell::new(creds.refresh_token.unwrap_or_default()),
            http,
        })
    }

    fn with_auth_retry<F, T>(&self, f: F) -> Result<T, SyncError>
    where
        F: Fn(&str) -> Result<T, SyncError>,
    {
        let token = self.access_token.borrow().clone();
        match f(&token) {
            Ok(val) => Ok(val),
            Err(SyncError::Backend { ref message }) if message.contains("401") => {
                self.refresh_access_token()?;
                let new_token = self.access_token.borrow().clone();
                f(&new_token)
            }
            Err(e) => Err(e),
        }
    }

    fn refresh_access_token(&self) -> Result<(), SyncError> {
        let refresh = self.refresh_token.borrow().clone();
        if refresh.is_empty() {
            return Err(SyncError::Backend {
                message: "No refresh token available".to_string(),
            });
        }

        let url = format!("{}/api/v1/auth/refresh", self.server_url);
        let resp = self
            .http
            .post(&url)
            .send_json(ureq::json!({
                "refresh_token": refresh,
            }))
            .map_err(|e| SyncError::Backend {
                message: format!("Refresh token request failed: {e}"),
            })?;

        let body: RefreshResponse = resp.into_json().map_err(|e| SyncError::Backend {
            message: format!("Failed to parse refresh response: {e}"),
        })?;

        *self.access_token.borrow_mut() = body.access_token.clone();
        *self.refresh_token.borrow_mut() = body.refresh_token.clone();

        // Persist to keychain
        if let Ok(store) = saku_crypto::keychain::SyncCredentialStore::new() {
            let mut creds = store.load().unwrap_or_default();
            creds.access_token = Some(body.access_token);
            creds.refresh_token = Some(body.refresh_token);
            let _ = store.store(&creds);
        }

        Ok(())
    }

    /// GET /kv/{tool}?cookie={cookie}&limit={limit}
    fn pull_entries(
        &self,
        tool: &str,
        cookie: Option<&str>,
        limit: i64,
    ) -> Result<GetEntriesResponse, SyncError> {
        self.with_auth_retry(|token| {
            let mut url = format!("{}/api/v1/kv/{}?limit={}", self.server_url, tool, limit);
            if let Some(c) = cookie {
                url.push_str(&format!("&cookie={}", c));
            }

            let resp = self
                .http
                .get(&url)
                .set("Authorization", &format!("Bearer {}", token))
                .call()
                .map_err(|e| SyncError::Backend {
                    message: format!("{e}"),
                })?;

            resp.into_json::<GetEntriesResponse>()
                .map_err(|e| SyncError::Backend {
                    message: format!("Failed to parse pull response: {e}"),
                })
        })
    }

    /// PUT /kv/{tool} with batch entries
    fn push_entries(
        &self,
        tool: &str,
        entries: &[BatchPutEntry],
    ) -> Result<BatchPutResponse, SyncError> {
        self.with_auth_retry(|token| {
            let url = format!("{}/api/v1/kv/{}", self.server_url, tool);
            let body = BatchPutRequest {
                entries: entries.to_vec(),
            };

            let resp = self
                .http
                .put(&url)
                .set("Authorization", &format!("Bearer {}", token))
                .send_json(&body)
                .map_err(|e| SyncError::Backend {
                    message: format!("{e}"),
                })?;

            resp.into_json::<BatchPutResponse>()
                .map_err(|e| SyncError::Backend {
                    message: format!("Failed to parse push response: {e}"),
                })
        })
    }
}

// ---------------------------------------------------------------------------
// Core sync logic
// ---------------------------------------------------------------------------

/// Run a full KV sync cycle: pull remote changes, merge, push dirty entries.
///
/// This implements the client side of per-entry encrypted KV sync:
/// 1. Load DirtyTracker from sidecar
/// 2. Derive master key from passphrase
/// 3. Pull remote entries since last cookie (paginated)
/// 4. Decrypt + merge each remote entry into local store (LWW)
/// 5. Reconcile renames + repair references + dedup task numbers
/// 6. Save merged store
/// 7. Push all dirty entries (encrypted)
/// 8. Update cookie + clear pushed dirty keys
/// 9. Save DirtyTracker
pub fn sync_kv(config: &KvSyncConfig) -> Result<KvSyncOutcome, SyncError> {
    let client = KvHttpClient::new(&config.server_url)?;
    let dirty_path = DirtyTracker::sidecar_path(&config.store_path);
    let mut tracker = DirtyTracker::load(&dirty_path)
        .map_err(|e| SyncError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

    // Derive master key
    let salt = derive_deterministic_salt(&config.passphrase);
    let master_key = derive_master_key(&config.passphrase, &salt)?;

    // Load local store
    let local_json = load_store_json(&config.store_path)?;
    let mut local_kv: KvStore = serde_json::from_value(local_json)?;

    // ------ PULL ------
    let mut pulled_count = 0;
    let mut last_cookie = tracker.last_cookie.clone();
    let is_initial_sync = last_cookie.is_none();

    loop {
        let resp = client.pull_entries(
            &config.tool,
            last_cookie.as_deref(),
            100,
        )?;

        for entry in &resp.entries {
            let blob_bytes = BASE64
                .decode(&entry.blob)
                .map_err(|e| SyncError::Backend {
                    message: format!("Invalid base64 in entry {}: {e}", entry.key),
                })?;

            let plaintext = decrypt_entry(&blob_bytes, &master_key)?;
            let remote_value: Value = serde_json::from_slice(&plaintext)?;

            // LWW merge: compare with local entry
            merge_single_entry(&mut local_kv, &entry.key, remote_value);
            pulled_count += 1;
        }

        last_cookie = Some(resp.cookie.clone());

        if !resp.has_more {
            break;
        }
    }

    // ------ POST-MERGE REPAIR ------
    if pulled_count > 0 {
        kv_store::reconcile_renames(&mut local_kv);
        kv_store::repair_references(&mut local_kv, &tdo_entity_schemas());
        fix_duplicate_task_numbers_kv(&mut local_kv.entries);
    }

    // On initial sync, mark all local entries dirty so they get pushed
    if is_initial_sync {
        let all_keys: Vec<String> = local_kv.entries.keys().cloned().collect();
        tracker.mark_dirty_many(all_keys);
    }

    // Save merged store
    save_store_json(&config.store_path, &local_kv)?;

    // ------ PUSH ------
    let dirty_keys: Vec<String> = tracker.dirty_keys.iter().cloned().collect();
    let mut pushed_count = 0;

    if !dirty_keys.is_empty() {
        // Build encrypted batch entries
        let mut batch: Vec<BatchPutEntry> = Vec::new();

        for key in &dirty_keys {
            if let Some(value) = local_kv.entries.get(key) {
                let plaintext = serde_json::to_vec(value)?;
                let blob = encrypt_entry(&plaintext, &master_key);
                let blob_b64 = BASE64.encode(&blob);
                batch.push(BatchPutEntry {
                    key: key.clone(),
                    blob: blob_b64,
                });
            }
            // If key was deleted from store (tombstone already removed), skip.
            // Server will keep its version until GC.
        }

        // Push in batches of 50
        for chunk in batch.chunks(50) {
            let resp = client.push_entries(&config.tool, chunk)?;
            pushed_count += chunk.len();
            // Update cookie from push response (it reflects the latest seq)
            last_cookie = Some(resp.cookie.clone());
        }

        // Clear dirty flags for pushed entries
        tracker.clear_dirty(&dirty_keys);
    }

    // Save cookie and tracker
    tracker.set_cookie(last_cookie);
    tracker.save(&dirty_path)
        .map_err(|e| SyncError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

    Ok(KvSyncOutcome {
        pulled: pulled_count,
        pushed: pushed_count,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Merge a single remote entry into the local KV store using LWW.
fn merge_single_entry(local: &mut KvStore, key: &str, remote_value: Value) {
    match local.entries.get(key) {
        Some(local_value) => {
            if kv_store::compare_modified_at(&remote_value, local_value)
                == std::cmp::Ordering::Greater
            {
                local.entries.insert(key.to_string(), remote_value);
            }
        }
        None => {
            local.entries.insert(key.to_string(), remote_value);
        }
    }
}

fn load_store_json(path: &Path) -> Result<Value, SyncError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No store yet — start with empty v9 store
            return Ok(serde_json::to_value(KvStore::new(9))?);
        }
        Err(e) => return Err(SyncError::Io(e)),
    };
    Ok(serde_json::from_str(&content)?)
}

fn save_store_json(path: &Path, store: &KvStore) -> Result<(), SyncError> {
    let json = serde_json::to_string_pretty(store)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    saku_storage::io::atomic_writer::atomic_write(path, &json)
        .map_err(|e| SyncError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_single_entry_remote_wins_when_newer() {
        let mut store = KvStore::new(9);
        store.entries.insert(
            "task/a".to_string(),
            json!({
                "title": "local",
                "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
            }),
        );

        let remote = json!({
            "title": "remote",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        });

        merge_single_entry(&mut store, "task/a", remote);
        assert_eq!(store.entries["task/a"]["title"], "remote");
    }

    #[test]
    fn merge_single_entry_local_wins_when_newer() {
        let mut store = KvStore::new(9);
        store.entries.insert(
            "task/a".to_string(),
            json!({
                "title": "local",
                "modified_at": {"wall_ms": 300, "lamport": 1, "device_id": "dev-a"}
            }),
        );

        let remote = json!({
            "title": "remote",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        });

        merge_single_entry(&mut store, "task/a", remote);
        assert_eq!(store.entries["task/a"]["title"], "local");
    }

    #[test]
    fn merge_single_entry_new_key_inserted() {
        let mut store = KvStore::new(9);

        let remote = json!({
            "title": "new task",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-b"}
        });

        merge_single_entry(&mut store, "task/new", remote);
        assert_eq!(store.entries["task/new"]["title"], "new task");
    }

    #[test]
    fn load_missing_store_returns_empty_kv() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let val = load_store_json(&path).unwrap();
        let kv: KvStore = serde_json::from_value(val).unwrap();
        assert_eq!(kv.version, 9);
        assert!(kv.entries.is_empty());
    }

    #[test]
    fn save_and_load_store_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("store.json");

        let mut store = KvStore::new(9);
        store.entries.insert(
            "task/a".to_string(),
            json!({"title": "test", "modified_at": {"wall_ms": 1, "lamport": 1, "device_id": "d"}}),
        );
        save_store_json(&path, &store).unwrap();

        let loaded_val = load_store_json(&path).unwrap();
        let loaded: KvStore = serde_json::from_value(loaded_val).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries["task/a"]["title"], "test");
    }
}
