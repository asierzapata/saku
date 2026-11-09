pub mod backend;
pub mod conflict;
pub mod error;
pub mod hash;
pub mod merkle;
pub mod state_db;
pub mod sync_engine;

#[cfg(feature = "server")]
pub mod config;

#[cfg(feature = "server")]
pub mod kv_sync;

pub use error::SyncError;
pub use sync_engine::{SyncConfig, SyncEngine, SyncOutcome, TrackedFile};

use backend::local_fs::LocalFsSyncBackend;
use std::path::Path;

/// Convenience function: build a `SyncEngine<LocalFsSyncBackend>`, run `sync()`,
/// and return the outcome. Intended for Phase 3 testing with a local directory
/// acting as the "remote".
pub fn try_flush_if_online(
    store_path: &Path,
    passphrase: &[u8],
    backend_root: &Path,
) -> Result<SyncOutcome, SyncError> {
    let backend = LocalFsSyncBackend::new(backend_root);

    // Build tracked files list — for now we only track the tdo store.json
    let store_path = store_path.to_path_buf();
    let tracked = vec![TrackedFile {
        file_key: "tdo/store.json".to_string(),
        tool: "tdo".to_string(),
        relative_path: "store.json".to_string(),
        local_path: store_path,
    }];

    let db_path = saku_storage::device::saku_data_dir()
        .map_err(SyncError::DeviceId)?
        .join("sync.db");

    let config = SyncConfig {
        db_path,
        passphrase: passphrase.to_vec(),
        tracked_files: tracked,
    };

    let mut engine = SyncEngine::new(config, backend)?;
    engine.sync()
}

/// Convenience function: build a `SyncEngine<ServerSyncBackend>`, run `sync()`,
/// and return the outcome. Used when the server feature is enabled and configured.
#[cfg(feature = "server")]
pub fn try_flush_if_online_server(
    store_path: &std::path::Path,
    passphrase: &[u8],
    server_url: &str,
    device_id: &str,
) -> Result<SyncOutcome, SyncError> {
    let backend = backend::server::ServerSyncBackend::new(server_url, device_id)?;

    let tracked = vec![TrackedFile {
        file_key: "tdo/store.json".to_string(),
        tool: "tdo".to_string(),
        relative_path: "store.json".to_string(),
        local_path: store_path.to_path_buf(),
    }];

    let db_path = saku_storage::device::saku_data_dir()
        .map_err(SyncError::DeviceId)?
        .join("sync.db");

    let config = SyncConfig {
        db_path,
        passphrase: passphrase.to_vec(),
        tracked_files: tracked,
    };

    let mut engine = SyncEngine::new(config, backend)?;
    engine.sync()
}

/// Convenience function: run per-entry KV sync against the server.
///
/// Loads the store from disk, pulls remote changes (paginated), merges via LWW,
/// pushes dirty entries, and saves the DirtyTracker sidecar.
#[cfg(feature = "server")]
pub fn try_kv_sync_server(
    store_path: &std::path::Path,
    passphrase: &[u8],
    server_url: &str,
    tool: &str,
) -> Result<kv_sync::KvSyncOutcome, SyncError> {
    let config = kv_sync::KvSyncConfig {
        server_url: server_url.to_string(),
        tool: tool.to_string(),
        passphrase: passphrase.to_vec(),
        store_path: store_path.to_path_buf(),
    };
    kv_sync::sync_kv(&config)
}

#[cfg(test)]
mod self_healing_tests {
    use saku_crypto::kdf::{derive_deterministic_salt, derive_master_key};
    use saku_storage::dirty_tracker::DirtyTracker;
    use saku_storage::kv_store::KvStore;
    use serde_json::json;

    #[test]
    fn skipped_entries_marked_dirty_for_healing() {
        let passphrase_a = b"device-a-passphrase";
        let passphrase_b = b"device-b-different";

        let salt_a = derive_deterministic_salt(passphrase_a);
        let mk_a = derive_master_key(passphrase_a, &salt_a).unwrap();

        let salt_b = derive_deterministic_salt(passphrase_b);
        let mk_b = derive_master_key(passphrase_b, &salt_b).unwrap();

        // Encrypt with key A, try to decrypt with key B — should fail
        let plaintext = br#"{"title":"test"}"#;
        let blob = saku_crypto::encrypt_entry(plaintext, &mk_a);
        assert!(saku_crypto::decrypt_entry(&blob, &mk_b).is_err());

        // Simulate self-healing: local data exists for this key → mark dirty
        let mut store = KvStore::new(9);
        store.entries.insert(
            "task/abc".to_string(),
            json!({"title": "local version"}),
        );

        let skipped_keys = vec!["task/abc".to_string()];
        let mut tracker = DirtyTracker::new();
        tracker.set_cookie(Some("5".to_string()));

        let healable: Vec<String> = skipped_keys
            .iter()
            .filter(|k| store.entries.contains_key(k.as_str()))
            .cloned()
            .collect();
        tracker.mark_dirty_many(&healable);

        assert!(tracker.is_dirty("task/abc"));
        assert_eq!(tracker.dirty_count(), 1);
    }

    #[test]
    fn skipped_entries_without_local_data_not_marked_dirty() {
        let store = KvStore::new(9);
        let mut tracker = DirtyTracker::new();

        let skipped_keys = vec!["task/unknown".to_string()];
        let healable: Vec<String> = skipped_keys
            .iter()
            .filter(|k| store.entries.contains_key(k.as_str()))
            .cloned()
            .collect();
        tracker.mark_dirty_many(&healable);

        assert!(!tracker.is_dirty("task/unknown"));
        assert_eq!(tracker.dirty_count(), 0);
    }
}
