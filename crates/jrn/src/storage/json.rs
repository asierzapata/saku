use std::path::PathBuf;

use serde_json::to_string_pretty;

use saku_storage::io::{
    atomic_writer::atomic_write,
    backup::{cleanup_old_backups, create_backup},
    file_lock::FileLock,
};

use crate::{
    models::store::{Store, StoredStore, CURRENT_VERSION},
    storage::{Storage, StorageError},
};

pub struct JsonFileStorage {
    path: PathBuf,
}

impl JsonFileStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Storage for JsonFileStorage {
    fn load(&self) -> Result<Store, StorageError> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                let data: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    })?;

                // Check version
                if let Some(version) = data.get("version").and_then(|v| v.as_u64())
                    && version as u32 > CURRENT_VERSION
                {
                    return Err(StorageError::FutureVersion(version as u32));
                }

                let stored_store: StoredStore =
                    serde_json::from_value(data).map_err(|e| StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    })?;

                Ok(Store::from_stored(stored_store))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(StorageError::LoadFailed {
                path: self.path.clone(),
                source: e,
            }),
        }
    }

    fn save(&self, store: &Store) -> Result<(), StorageError> {
        let stored_store = store.to_stored();

        let json = to_string_pretty(&stored_store)
            .map_err(|e| StorageError::SerializeFailed { source: e })?;

        let lock_file_path = self.path.with_extension("lock");
        let lock = FileLock::acquire(&lock_file_path).map_err(|e| StorageError::SaveFailed {
            path: lock_file_path.clone(),
            source: match e {
                saku_storage::error::IoError::LockFailed { source, .. } => source,
                _ => std::io::Error::other(e.to_string()),
            },
        })?;

        create_backup(&self.path).map_err(|e| StorageError::BackupFailed {
            path: self.path.clone(),
            source: match e {
                saku_storage::error::IoError::BackupFailed { source, .. } => source,
                _ => std::io::Error::other(e.to_string()),
            },
        })?;

        cleanup_old_backups(&self.path, 5).map_err(|e| StorageError::CleanupFailed {
            dir: self.path.clone(),
            source: match e {
                saku_storage::error::IoError::CleanupFailed { source, .. } => source,
                _ => std::io::Error::other(e.to_string()),
            },
        })?;

        atomic_write(&self.path, &json).map_err(|e| StorageError::SaveFailed {
            path: self.path.clone(),
            source: match e {
                saku_storage::error::IoError::WriteFailed { source, .. } => source,
                saku_storage::error::IoError::RenameFailed { source, .. } => source,
                _ => std::io::Error::other(e.to_string()),
            },
        })?;

        lock.release().map_err(|e| StorageError::SaveFailed {
            path: self.path.clone(),
            source: match e {
                saku_storage::error::IoError::UnlockFailed { source, .. } => source,
                _ => std::io::Error::other(e.to_string()),
            },
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::entry::{Entry, EntryKind};

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("store.json");
        let storage = JsonFileStorage::new(store_path);

        let mut store = Store::default();
        store.add_entry(Entry {
            storage_key_suffix: "abc123".into(),
            body: "Test entry".into(),
            date: "2026-03-01".into(),
            time: "10:00:00".into(),
            kind: EntryKind::Log,
            ..Entry::default()
        });

        storage.save(&store).unwrap();
        let loaded = storage.load().unwrap();

        assert_eq!(loaded.entries.len(), 1);
        let entry = loaded.get_entry_by_number(1).unwrap();
        assert_eq!(entry.body, "Test entry");
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("nonexistent.json");
        let storage = JsonFileStorage::new(store_path);

        let store = storage.load().unwrap();
        assert_eq!(store.entries.len(), 0);
        assert_eq!(store.next_entry_number(), 1);
    }

    #[test]
    fn load_invalid_json_returns_parse_failed() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("invalid.json");
        std::fs::write(&store_path, "{ not valid json }").unwrap();

        let storage = JsonFileStorage::new(store_path);
        let result = storage.load();

        assert!(matches!(result, Err(StorageError::ParseFailed { .. })));
    }

    #[test]
    fn load_future_version_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("future.json");
        std::fs::write(
            &store_path,
            r#"{"version": 999, "entries": {}}"#,
        )
        .unwrap();

        let storage = JsonFileStorage::new(store_path);
        let result = storage.load();

        assert!(matches!(result, Err(StorageError::FutureVersion(999))));
    }
}
