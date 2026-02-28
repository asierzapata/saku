use crate::{
    models::{area::Area, store::Store},
    storage::{Storage, StorageError},
};
use saku_storage::entity::Entity;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CreateAreaError {
    #[error("Area with name '{}' already exists", .0)]
    AreaAlreadyExists(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct CreateAreaParameters {
    pub name: String,
}

pub fn create_area(
    store: &mut Store,
    storage: &impl Storage,
    parameters: CreateAreaParameters,
) -> Result<Area, CreateAreaError> {
    let already_exists = store
        .get_active_areas()
        .any(|a| a.name.to_lowercase() == parameters.name.to_lowercase());

    if already_exists {
        return Err(CreateAreaError::AreaAlreadyExists(parameters.name));
    }

    let area = Area {
        name: parameters.name,
        deleted_at: None,
        modified_at: crate::sync_clock::next_modified_at(),
        renamed_to: None,
        previous_key: None,
    };

    let area_key = store.add_area(area);

    storage.save(store)?;

    Ok(store.get_area(&area_key).unwrap().clone())
}

#[derive(Debug, Error)]
pub enum DeleteAreaError {
    #[error("Area with name '{}' not found", .0)]
    AreaNotFound(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct DeleteAreaParameters {
    pub name: String,
}

pub struct DeleteAreaResult {
    pub area: Area,
    pub cascaded_projects_count: usize,
    pub cascaded_tasks_count: usize,
}

pub fn delete_area(
    store: &mut Store,
    storage: &impl Storage,
    parameters: DeleteAreaParameters,
) -> Result<DeleteAreaResult, DeleteAreaError> {
    // Fuzzy match to find area
    let matching_areas: Vec<_> = store
        .get_active_areas()
        .filter(|a| {
            a.name
                .to_lowercase()
                .contains(&parameters.name.to_lowercase())
        })
        .collect();

    let area = match matching_areas.len() {
        0 => return Err(DeleteAreaError::AreaNotFound(parameters.name)),
        1 => matching_areas[0],
        _ => {
            // If ambiguous, require exact match or fail
            return Err(DeleteAreaError::AreaNotFound(parameters.name));
        }
    };

    let area_key = area.storage_key();
    let now = jiff::Timestamp::now();

    // Cascade delete: Find all projects in this area
    let project_keys_to_delete: Vec<String> = store
        .get_projects_for_area(&area_key)
        .filter(|p| p.deleted_at.is_none())
        .map(|p| p.storage_key())
        .collect();

    let mut total_tasks_deleted = 0;

    // For each project, cascade delete its tasks
    for project_key in &project_keys_to_delete {
        let task_keys: Vec<String> = store
            .get_tasks_for_project(project_key)
            .filter(|t| t.deleted_at.is_none())
            .map(|t| t.storage_key())
            .collect();

        total_tasks_deleted += task_keys.len();

        for task_key in task_keys {
            if let Some(task) = store.get_task_mut(&task_key) {
                task.deleted_at = Some(now);
                task.modified_at = crate::sync_clock::next_modified_at();
            }
        }
    }

    // Mark all projects in this area as deleted
    for project_key in &project_keys_to_delete {
        if let Some(project) = store.get_project_mut(project_key) {
            project.deleted_at = Some(now);
            project.modified_at = crate::sync_clock::next_modified_at();
        }
    }

    // Also delete tasks directly under this area (not in a project)
    let direct_task_keys: Vec<String> = store
        .get_tasks_for_area(&area_key)
        .filter(|t| t.deleted_at.is_none())
        .map(|t| t.storage_key())
        .collect();

    total_tasks_deleted += direct_task_keys.len();

    for task_key in direct_task_keys {
        if let Some(task) = store.get_task_mut(&task_key) {
            task.deleted_at = Some(now);
            task.modified_at = crate::sync_clock::next_modified_at();
        }
    }

    // Mark area as deleted
    if let Some(area) = store.get_area_mut(&area_key) {
        area.deleted_at = Some(now);
        area.modified_at = crate::sync_clock::next_modified_at();
    }

    // Persist to storage
    storage.save(store)?;

    Ok(DeleteAreaResult {
        area: store.get_area(&area_key).unwrap().clone(),
        cascaded_projects_count: project_keys_to_delete.len(),
        cascaded_tasks_count: total_tasks_deleted,
    })
}

#[derive(Debug, Error)]
pub enum EditAreaError {
    #[error("Area '{0}' not found")]
    AreaNotFound(String),

    #[error("Ambiguous area name '{name}'. Multiple areas found: {names}", name = .0, names = .1.join(", "))]
    AmbiguousAreaName(String, Vec<String>),

    #[error("Area with name '{0}' already exists")]
    AreaAlreadyExists(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct EditAreaParameters {
    pub name: String,
    pub new_name: String,
}

pub fn edit_area(
    store: &mut Store,
    storage: &impl Storage,
    parameters: EditAreaParameters,
) -> Result<Area, EditAreaError> {
    // Fuzzy match to find area
    let matching_areas: Vec<_> = store
        .get_active_areas()
        .filter(|a| {
            a.name
                .to_lowercase()
                .contains(&parameters.name.to_lowercase())
        })
        .collect();

    let area = match matching_areas.len() {
        0 => return Err(EditAreaError::AreaNotFound(parameters.name)),
        1 => matching_areas[0],
        _ => {
            // Multiple matches - return error with all matching names
            let names: Vec<String> = matching_areas.iter().map(|a| a.name.clone()).collect();
            return Err(EditAreaError::AmbiguousAreaName(parameters.name, names));
        }
    };

    let old_key = area.storage_key();
    let old_name = area.name.clone();
    let new_key = format!("area/{}", parameters.new_name.to_lowercase());

    // Check if new name already exists (case-insensitive, excluding current area)
    let name_conflict = old_key != new_key
        && store
            .get_active_areas()
            .any(|a| a.name.to_lowercase() == parameters.new_name.to_lowercase());

    if name_conflict {
        return Err(EditAreaError::AreaAlreadyExists(parameters.new_name));
    }

    // Drop the borrow on store before calling rename_area (mutable borrow)
    drop(matching_areas);

    // Use store.rename_area to handle tombstone + create + reference updates
    store
        .rename_area(&old_name, &parameters.new_name)
        .ok_or(EditAreaError::AreaNotFound(parameters.name))?;

    // Persist to storage
    storage.save(store)?;

    Ok(store.get_area(&new_key).unwrap().clone())
}

#[derive(Debug, Error)]
pub enum RestoreAreaError {
    #[error("Area '{0}' not found")]
    AreaNotFound(String),

    #[error("Area '{0}' is not deleted")]
    AreaNotDeleted(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct RestoreAreaParameters {
    pub name: String,
}

pub fn restore_area(
    store: &mut Store,
    storage: &impl Storage,
    parameters: RestoreAreaParameters,
) -> Result<Area, RestoreAreaError> {
    // Find deleted area by name
    let matching_areas: Vec<_> = store
        .get_deleted_areas()
        .filter(|a| {
            a.name
                .to_lowercase()
                .contains(&parameters.name.to_lowercase())
        })
        .collect();

    let area = match matching_areas.len() {
        0 => return Err(RestoreAreaError::AreaNotFound(parameters.name)),
        1 => matching_areas[0],
        _ => return Err(RestoreAreaError::AreaNotFound(parameters.name)),
    };

    let area_key = area.storage_key();

    // Restore area (does NOT auto-restore projects/tasks - user must restore them separately)
    if let Some(area) = store.get_area_mut(&area_key) {
        area.deleted_at = None;
        area.modified_at = crate::sync_clock::next_modified_at();
    }

    // Persist to storage
    storage.save(store)?;

    Ok(store.get_area(&area_key).unwrap().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{project::Project, task::Task};
    use std::cell::RefCell;

    // Mock storage implementation for testing
    struct MockStorage {
        store: RefCell<Store>,
        save_count: RefCell<usize>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                store: RefCell::new(Store::default()),
                save_count: RefCell::new(0),
            }
        }

        fn save_count(&self) -> usize {
            *self.save_count.borrow()
        }
    }

    impl Storage for MockStorage {
        fn load(&self) -> Result<Store, StorageError> {
            Ok(self.store.borrow().clone())
        }

        fn save(&self, store: &Store) -> Result<(), StorageError> {
            *self.store.borrow_mut() = store.clone();
            *self.save_count.borrow_mut() += 1;
            Ok(())
        }
    }

    // Helper functions for test fixtures
    fn create_test_project(store: &mut Store, name: &str, area_key: Option<String>) -> Project {
        let project = Project {
            name: name.to_string(),
            area_key,
            created_at: jiff::Timestamp::now(),
            ..Project::default()
        };
        let key = store.add_project(project);
        store.get_project(&key).unwrap().clone()
    }

    fn create_test_task(
        store: &mut Store,
        title: &str,
        project_key: Option<String>,
        area_key: Option<String>,
    ) -> Task {
        // Use title as a unique seed so rapid calls don't collide on the same millisecond.
        let task = Task {
            storage_key_suffix: saku_storage::key_gen::generate_task_key(
                title,
                store.next_task_number() as i64,
            ),
            task_number: 0,
            title: title.to_string(),
            project_key,
            area_key,
            created_at: jiff::Timestamp::now(),
            ..Task::default()
        };
        store.add_task(task);
        let number = store.next_task_number() - 1;
        store.get_task_by_number(number).unwrap().clone()
    }

    // ============================================================================
    // create_area tests
    // ============================================================================

    #[test]
    fn test_create_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Work".to_string(),
            },
        );

        assert!(result.is_ok());
        let area = result.unwrap();
        assert_eq!(area.name, "Work");
        assert!(area.deleted_at.is_none());
        assert_eq!(storage.save_count(), 1);
    }

    // ============================================================================
    // delete_area tests
    // ============================================================================

    #[test]
    fn test_delete_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Work".to_string(),
            },
        )
        .unwrap();

        let result = delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "Work".to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert!(deleted.area.deleted_at.is_some());
        assert_eq!(deleted.cascaded_projects_count, 0);
        assert_eq!(deleted.cascaded_tasks_count, 0);
    }

    #[test]
    fn test_delete_area_cascades_to_projects() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let area = create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Work".to_string(),
            },
        )
        .unwrap();

        let area_key = area.storage_key();

        // Create projects in this area
        create_test_project(&mut store, "Project 1", Some(area_key.clone()));
        create_test_project(&mut store, "Project 2", Some(area_key.clone()));

        let result = delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "Work".to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted.cascaded_projects_count, 2);

        // Verify projects are deleted
        let active_projects: Vec<_> = store.get_active_projects().collect();
        assert_eq!(active_projects.len(), 0);
    }

    #[test]
    fn test_delete_area_cascades_to_tasks() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let area = create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Work".to_string(),
            },
        )
        .unwrap();

        let area_key = area.storage_key();

        let project = create_test_project(&mut store, "Project 1", Some(area_key.clone()));
        let project_key = project.storage_key();

        // Create tasks in the project
        create_test_task(&mut store, "Task 1", Some(project_key.clone()), None);
        create_test_task(&mut store, "Task 2", Some(project_key.clone()), None);

        // Create tasks directly under the area (no project)
        create_test_task(&mut store, "Direct Task 1", None, Some(area_key.clone()));
        create_test_task(&mut store, "Direct Task 2", None, Some(area_key.clone()));

        let result = delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "Work".to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted.cascaded_projects_count, 1);
        assert_eq!(deleted.cascaded_tasks_count, 4); // 2 in project + 2 direct

        // Verify all tasks are deleted
        let active_tasks: Vec<_> = store.get_active_tasks().collect();
        assert_eq!(active_tasks.len(), 0);
    }

    #[test]
    fn test_delete_area_returns_counts() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let area = create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Personal".to_string(),
            },
        )
        .unwrap();

        let area_key = area.storage_key();

        let project1 = create_test_project(&mut store, "Project 1", Some(area_key.clone()));
        let project2 = create_test_project(&mut store, "Project 2", Some(area_key.clone()));
        let project1_key = project1.storage_key();
        let project2_key = project2.storage_key();

        // 2 tasks in project1, 1 task in project2, 1 direct task
        create_test_task(&mut store, "Task 1.1", Some(project1_key.clone()), None);
        create_test_task(&mut store, "Task 1.2", Some(project1_key.clone()), None);
        create_test_task(&mut store, "Task 2.1", Some(project2_key.clone()), None);
        create_test_task(&mut store, "Direct Task", None, Some(area_key.clone()));

        let result = delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "Personal".to_string(),
            },
        )
        .unwrap();

        assert_eq!(result.cascaded_projects_count, 2);
        assert_eq!(result.cascaded_tasks_count, 4);
    }

    #[test]
    fn test_delete_area_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "NonExistent".to_string(),
            },
        );

        assert!(matches!(result, Err(DeleteAreaError::AreaNotFound(_))));
    }

    // ============================================================================
    // restore_area tests
    // ============================================================================

    #[test]
    fn test_restore_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Work".to_string(),
            },
        )
        .unwrap();

        // Delete first
        delete_area(
            &mut store,
            &storage,
            DeleteAreaParameters {
                name: "Work".to_string(),
            },
        )
        .unwrap();

        // Restore
        let result = restore_area(
            &mut store,
            &storage,
            RestoreAreaParameters {
                name: "Work".to_string(),
            },
        );

        assert!(result.is_ok());
        let restored = result.unwrap();
        assert!(restored.deleted_at.is_none());
    }

    #[test]
    fn test_restore_area_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = restore_area(
            &mut store,
            &storage,
            RestoreAreaParameters {
                name: "NonExistent".to_string(),
            },
        );

        assert!(matches!(result, Err(RestoreAreaError::AreaNotFound(_))));
    }

    #[test]
    fn test_restore_area_not_deleted() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_area(
            &mut store,
            &storage,
            CreateAreaParameters {
                name: "Active Area".to_string(),
            },
        )
        .unwrap();

        // Try to restore an active (non-deleted) area
        let result = restore_area(
            &mut store,
            &storage,
            RestoreAreaParameters {
                name: "Active".to_string(),
            },
        );

        // Should not find it in deleted areas
        assert!(matches!(result, Err(RestoreAreaError::AreaNotFound(_))));
    }
}
