use std::path::PathBuf;

use serde_json::to_string_pretty;

use saku_storage::io::{
    atomic_writer::atomic_write,
    backup::{cleanup_old_backups, create_backup},
    file_lock::FileLock,
};

use crate::{
    models::store::{Store, StoredStore},
    storage::{Storage, StorageError},
};

#[cfg(feature = "logging")]
use tracing::{debug, error, info, instrument};

pub struct JsonFileStorage {
    path: PathBuf,
}

impl JsonFileStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Storage for JsonFileStorage {
    #[cfg_attr(feature = "logging", instrument(skip(self), fields(path = %self.path.display())))]
    fn load(&self) -> Result<Store, StorageError> {
        use crate::models::store::CURRENT_VERSION;
        use crate::storage::migrations::{apply_migrations, detect_version};

        #[cfg(feature = "logging")]
        debug!("Loading store from disk");

        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                let file_version = detect_version(&content)?;

                if file_version > CURRENT_VERSION {
                    #[cfg(feature = "logging")]
                    error!(
                        file_version = file_version,
                        current_version = CURRENT_VERSION,
                        "Store file is from a future version"
                    );
                    return Err(StorageError::FutureVersion(file_version));
                }

                let mut data: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
                    #[cfg(feature = "logging")]
                    error!(error = %e, "Failed to parse store JSON");
                    StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    }
                })?;

                if file_version < CURRENT_VERSION {
                    #[cfg(feature = "logging")]
                    info!(
                        from_version = file_version,
                        to_version = CURRENT_VERSION,
                        "Applying migrations"
                    );
                    data = apply_migrations(data, file_version, CURRENT_VERSION)?;
                }

                if let Some(obj) = data.as_object_mut() {
                    obj.insert("version".to_string(), serde_json::json!(CURRENT_VERSION));
                }

                let stored_store: StoredStore = serde_json::from_value(data).map_err(|e| {
                    #[cfg(feature = "logging")]
                    error!(error = %e, "Failed to deserialize store");
                    StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    }
                })?;

                // Convert from storage format to working format
                let store = Store::from_stored(stored_store);

                #[cfg(feature = "logging")]
                info!(
                    tasks = store.tasks.len(),
                    projects = store.projects.len(),
                    areas = store.areas.len(),
                    "Store loaded successfully"
                );

                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(feature = "logging")]
                info!("Store file not found, creating new store");
                Ok(Store::default())
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                error!(error = %e, "Failed to read store file");
                Err(StorageError::LoadFailed {
                    path: self.path.clone(),
                    source: e,
                })
            }
        }
    }

    #[cfg_attr(feature = "logging", instrument(skip(self, store), fields(path = %self.path.display())))]
    fn save(&self, store: &Store) -> Result<(), StorageError> {
        #[cfg(feature = "logging")]
        debug!(
            tasks = store.tasks.len(),
            projects = store.projects.len(),
            areas = store.areas.len(),
            "Saving store to disk"
        );

        // Convert from working format to storage format
        let stored_store = store.to_stored();

        let json = to_string_pretty(&stored_store).map_err(|e| {
            #[cfg(feature = "logging")]
            error!(error = %e, "Failed to serialize store");
            StorageError::SerializeFailed { source: e }
        })?;

        #[cfg(feature = "logging")]
        debug!("Acquiring file lock");

        let lock_file_path = self.path.with_extension("lock");
        let lock = FileLock::acquire(&lock_file_path).map_err(|e| {
            #[cfg(feature = "logging")]
            error!(error = %e, "Failed to acquire lock");
            StorageError::SaveFailed {
                path: lock_file_path.clone(),
                source: match e {
                    saku_storage::error::IoError::LockFailed { source, .. } => source,
                    _ => std::io::Error::other(e.to_string()),
                },
            }
        })?;

        #[cfg(feature = "logging")]
        debug!("Creating backup");

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

        #[cfg(feature = "logging")]
        debug!("Writing store file atomically");

        // Atomic write: write to temp file + rename
        atomic_write(&self.path, &json).map_err(|e| {
            #[cfg(feature = "logging")]
            error!(error = %e, "Failed atomic write");
            StorageError::SaveFailed {
                path: self.path.clone(),
                source: match e {
                    saku_storage::error::IoError::WriteFailed { source, .. } => source,
                    saku_storage::error::IoError::RenameFailed { source, .. } => source,
                    _ => std::io::Error::other(e.to_string()),
                },
            }
        })?;

        lock.release().map_err(|e| {
            #[cfg(feature = "logging")]
            error!(error = %e, "Failed to release lock");
            StorageError::SaveFailed {
                path: self.path.clone(),
                source: match e {
                    saku_storage::error::IoError::UnlockFailed { source, .. } => source,
                    _ => std::io::Error::other(e.to_string()),
                },
            }
        })?;

        #[cfg(feature = "logging")]
        info!("Store saved successfully");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use crate::{
        models::{area::Area, project::Project, store::Store, task::Task},
        storage::json::JsonFileStorage,
    };

    #[test]
    fn test_save_and_load() {
        let area = Area {
            name: String::from("Some Area"),
            ..Area::default()
        };
        let area_id = area.id;

        let project = Project {
            area_id: Some(area_id),
            name: String::from("Some Project"),
            ..Project::default()
        };
        let project_id = project.id;

        let task = Task {
            title: String::from("Some Task"),
            project_id: Some(project_id),
            ..Task::default()
        };
        let task_id = task.id;

        let mut store = Store::default();
        store.add_area(area);
        store.add_project(project);
        store.add_task(task);

        let json_file_storage = JsonFileStorage {
            path: PathBuf::from("/tmp/test_store.json"),
        };
        if json_file_storage.save(&store).is_err() {
            panic!("Should correctly save the store");
        }
        match json_file_storage.load() {
            Ok(loaded_store) => {
                assert_eq!(loaded_store.get_area(area_id).unwrap().id, area_id);
                assert_eq!(loaded_store.get_project(project_id).unwrap().id, project_id);
                assert_eq!(loaded_store.get_task(task_id).unwrap().id, task_id);
                assert_eq!(loaded_store.get_task(task_id).unwrap().task_number, 1);
                assert_eq!(loaded_store.next_task_number, 2);
            }
            Err(_) => panic!("Should correctly load the saved store"),
        }
    }

    #[test]
    fn test_load_invalid_json() {
        let path = PathBuf::from("/tmp/invalid_store.json");

        std::fs::write(&path, "{ this is not valid json }").unwrap();

        let storage = JsonFileStorage::new(path);
        let result = storage.load();

        match result {
            Err(StorageError::ParseFailed { .. }) => {}
            _ => panic!("Expected ParseFailed error, got something else"),
        }
    }

    #[test]
    fn test_load_v1_without_version_field() {
        let path = PathBuf::from("/tmp/v1_store.json");
        let old_json = r#"{
            "tasks": [],
            "projects": [],
            "areas": []
        }"#;

        std::fs::write(&path, old_json).unwrap();

        let storage = JsonFileStorage::new(path);
        let result = storage.load();

        match result {
            Ok(store) => {
                assert_eq!(store.version, crate::models::store::CURRENT_VERSION);
                assert_eq!(store.next_task_number, 1);
            }
            Err(e) => panic!("Expected successful load, got error: {:?}", e),
        }
    }

    #[test]
    fn test_load_future_version() {
        let path = PathBuf::from("/tmp/future_store.json");
        let future_json = r#"{
            "version": 999,
            "tasks": [],
            "projects": [],
            "areas": []
        }"#;

        std::fs::write(&path, future_json).unwrap();

        let storage = JsonFileStorage::new(path);
        let result = storage.load();

        match result {
            Err(StorageError::FutureVersion(999)) => {
                // Expected: should fail with FutureVersion error
            }
            _ => panic!("Expected FutureVersion(999) error"),
        }
    }

    #[test]
    fn test_backup_creation_and_cleanup() {
        let test_dir = PathBuf::from("/tmp/tdo_backup_test");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let store_path = test_dir.join("store.json");
        let storage = JsonFileStorage::new(store_path.clone());

        for i in 1..=7 {
            let mut store = Store {
                version: i,
                ..Store::default()
            };

            // Add a unique task to make each save different
            let task = Task {
                title: format!("Task {}", i),
                ..Task::default()
            };
            store.add_task(task);

            storage.save(&store).unwrap();

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let backups_dir = test_dir.join("backups");
        let backup_count = fs::read_dir(&backups_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.metadata().map(|m| m.is_file()).unwrap_or(false))
            .count();

        assert_eq!(backup_count, 5, "Should keep exactly 5 backups");

        fs::remove_dir_all(&test_dir).unwrap();
    }

    #[test]
    fn test_backup_directory_created_on_second_save() {
        let test_dir = PathBuf::from("/tmp/tdo_backup_dir_test");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let store_path = test_dir.join("store.json");
        let storage = JsonFileStorage::new(store_path.clone());

        let backups_dir = test_dir.join("backups");
        assert!(!backups_dir.exists(), "Backups dir should not exist yet");

        let store = Store::default();
        storage.save(&store).unwrap();

        assert!(
            !backups_dir.exists(),
            "Backups dir should not exist after first save"
        );

        let mut store2 = Store {
            version: 2,
            ..Store::default()
        };

        // Add a task to make it different from first save
        let task = Task {
            title: String::from("Second save task"),
            ..Task::default()
        };
        store2.add_task(task);

        storage.save(&store2).unwrap();

        assert!(
            backups_dir.exists(),
            "Backups dir should be created on second save"
        );
        assert!(backups_dir.is_dir(), "Backups path should be a directory");

        fs::remove_dir_all(&test_dir).unwrap();
    }

    #[test]
    fn test_v1_to_v2_migration_backfills_task_numbers() {
        let path = PathBuf::from("/tmp/v1_migration_test.json");
        let v1_json = r#"{
            "tasks": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Second task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Inbox"},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-02T00:00:00Z"
                },
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "title": "First task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Inbox"},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-01T00:00:00Z"
                }
            ],
            "projects": [],
            "areas": []
        }"#;

        std::fs::write(&path, v1_json).unwrap();
        let storage = JsonFileStorage::new(path);
        let store = storage.load().expect("Migration should succeed");

        assert_eq!(store.version, 8);
        assert_eq!(store.next_task_number, 3);

        // "First task" (earlier created_at) gets task_number 1
        let first = store.get_task_by_number(1).expect("Task #1 should exist");
        assert_eq!(first.title, "First task");

        // "Second task" (later created_at) gets task_number 2
        let second = store.get_task_by_number(2).expect("Task #2 should exist");
        assert_eq!(second.title, "Second task");
    }

    #[test]
    fn test_task_number_auto_increments() {
        let mut store = Store::default();
        assert_eq!(store.next_task_number, 1);

        store.add_task(Task {
            title: "First".into(),
            ..Task::default()
        });
        assert_eq!(store.next_task_number, 2);
        assert_eq!(store.get_task_by_number(1).unwrap().title, "First");

        store.add_task(Task {
            title: "Second".into(),
            ..Task::default()
        });
        assert_eq!(store.next_task_number, 3);
        assert_eq!(store.get_task_by_number(2).unwrap().title, "Second");
    }

    #[test]
    fn test_get_task_by_number_not_found() {
        let store = Store::default();
        assert!(store.get_task_by_number(1).is_none());
        assert!(store.get_task_by_number(999).is_none());
    }
}
