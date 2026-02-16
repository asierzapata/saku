use std::{
    fs::{self, OpenOptions, rename, write},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde_json::to_string_pretty;
use uuid::Uuid;

use crate::{
    models::store::{Store, StoredStore},
    storage::{Storage, StorageError},
};

pub struct JsonFileStorage {
    path: PathBuf,
}

impl JsonFileStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn create_backup_dir(&self) -> Result<(), StorageError> {
        let backups_dir = self.get_backup_dir();
        fs::create_dir(&backups_dir).map_err(|e| StorageError::BackupFailed {
            path: backups_dir,
            source: e,
        })?;
        Ok(())
    }

    fn create_backup(&self) -> Result<u64, StorageError> {
        let file_exists = fs::exists(&self.path).map_err(|e| StorageError::BackupFailed {
            path: self.path.clone(),
            source: e,
        })?;
        if !file_exists {
            return Ok(0);
        }

        let backup_path = self.get_backup_path();
        let copy_result = fs::copy(&self.path, &backup_path);
        match copy_result {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.create_backup_dir()?;
                self.create_backup()
            }
            Err(e) => Err(StorageError::BackupFailed {
                path: backup_path,
                source: e,
            }),
            Ok(bytes) => Ok(bytes),
        }
    }

    fn cleanup_old_backups(&self) -> Result<(), StorageError> {
        let backup_dir = self.get_backup_dir();
        let backup_dir_exists =
            fs::exists(&backup_dir).map_err(|e| StorageError::CleanupFailed {
                dir: backup_dir.clone(),
                source: e,
            })?;
        if !backup_dir_exists {
            return Ok(());
        }

        let mut file_entries = fs::read_dir(&backup_dir)
            .map_err(|e| StorageError::CleanupFailed {
                dir: backup_dir.clone(),
                source: e,
            })?
            .flatten()
            .filter(|entry| entry.metadata().map(|m| m.is_file()).unwrap_or(false))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();

        file_entries.sort();

        let number_of_files_to_delete = match file_entries.len() {
            x if x > 5 => x - 5,
            _ => 0,
        };

        if number_of_files_to_delete == 0 {
            return Ok(());
        }

        for file_path in &file_entries[0..number_of_files_to_delete] {
            fs::remove_file(file_path).map_err(|e| StorageError::CleanupFailed {
                dir: backup_dir.clone(),
                source: e,
            })?;
        }

        Ok(())
    }

    fn get_backup_dir(&self) -> PathBuf {
        let parent_store_path = self.path.parent().unwrap_or(Path::new("."));
        parent_store_path.join("backups")
    }

    fn get_backup_path(&self) -> PathBuf {
        let backups_dir = self.get_backup_dir();

        let timestamp = jiff::Timestamp::now().to_string();
        let filename = format!("{:?}-{}", self.path.file_name(), timestamp);

        backups_dir.join(filename)
    }
}

impl Storage for JsonFileStorage {
    fn load(&self) -> Result<Store, StorageError> {
        use crate::models::store::CURRENT_VERSION;
        use crate::storage::migrations::{apply_migrations, detect_version};

        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                let file_version = detect_version(&content)?;

                if file_version > CURRENT_VERSION {
                    return Err(StorageError::FutureVersion(file_version));
                }

                let mut data: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    })?;

                if file_version < CURRENT_VERSION {
                    data = apply_migrations(data, file_version, CURRENT_VERSION)?;
                }

                if let Some(obj) = data.as_object_mut() {
                    obj.insert("version".to_string(), serde_json::json!(CURRENT_VERSION));
                }

                let stored_store: StoredStore =
                    serde_json::from_value(data).map_err(|e| StorageError::ParseFailed {
                        path: self.path.clone(),
                        source: e,
                    })?;

                // Convert from storage format to working format
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
        // Convert from working format to storage format
        let stored_store = store.to_stored();

        let json = to_string_pretty(&stored_store)
            .map_err(|e| StorageError::SerializeFailed { source: e })?;

        let unique_temp = format!("{}.tmp.{}", self.path.display(), Uuid::new_v4());
        let temp_path = PathBuf::from(&unique_temp);
        write(&temp_path, json).map_err(|e| StorageError::SaveFailed {
            path: temp_path.clone(),
            source: e,
        })?;

        let lock_file_path = self.path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&lock_file_path)
            .map_err(|e| StorageError::SaveFailed {
                path: lock_file_path.clone(),
                source: e,
            })?;
        lock_file
            .lock_exclusive()
            .map_err(|e| StorageError::SaveFailed {
                path: lock_file_path,
                source: e,
            })?;

        self.create_backup()?;
        self.cleanup_old_backups()?;

        rename(&temp_path, &self.path).map_err(|e| StorageError::SaveFailed {
            path: self.path.clone(),
            source: e,
        })?;

        lock_file.unlock().map_err(|e| StorageError::SaveFailed {
            path: self.path.clone(),
            source: e,
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        if let Err(_) = json_file_storage.save(&store) {
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
            let mut store = Store::default();
            store.version = i;

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

        let mut store2 = Store::default();
        store2.version = 2;

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

        assert_eq!(store.version, 2);
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
