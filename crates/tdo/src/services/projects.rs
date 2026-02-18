use crate::{
    models::{project::Project, store::Store},
    storage::{Storage, StorageError},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateProjectError {
    #[error("Area with name '{}' not found", .0)]
    AreaNotFound(String),

    #[error("Area name is ambiguous. Multiple areas found: {}", .0.join(", "))]
    AmbiguousAreaName(Vec<String>),

    #[error("Project with name '{}' already exists", .0)]
    ProjectAlreadyExists(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct CreateProjectParameters {
    pub name: String,
    pub area: Option<String>,
}

pub fn create_project(
    store: &mut Store,
    storage: &impl Storage,
    parameters: CreateProjectParameters,
) -> Result<Project, CreateProjectError> {
    let already_exists = store
        .get_active_projects()
        .any(|p| p.name.to_lowercase() == parameters.name.to_lowercase());

    if already_exists {
        return Err(CreateProjectError::ProjectAlreadyExists(parameters.name));
    }

    let area_id = match parameters.area {
        Some(area_name) => {
            let matching: Vec<_> = store
                .get_active_areas()
                .filter(|a| a.name.to_lowercase().contains(&area_name.to_lowercase()))
                .collect();
            Some(match matching.len() {
                0 => return Err(CreateProjectError::AreaNotFound(area_name)),
                1 => matching[0].id,
                _ => {
                    let names = matching.iter().map(|a| a.name.clone()).collect();
                    return Err(CreateProjectError::AmbiguousAreaName(names));
                }
            })
        }
        None => None,
    };

    let project = Project {
        id: Uuid::new_v4(),
        name: parameters.name,
        created_at: jiff::Timestamp::now(),
        area_id,
        ..Project::default()
    };

    let project_id = project.id;

    store.add_project(project);

    storage.save(store)?;

    Ok(store.get_project(project_id).unwrap().clone())
}

#[derive(Debug, Error)]
pub enum DeleteProjectError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project '{0}' is already deleted")]
    ProjectAlreadyDeleted(String),

    #[error("Project name is ambiguous. Multiple projects found: {}", .0.join(", "))]
    AmbiguousProjectName(Vec<String>),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct DeleteProjectParameters {
    pub name: String,
}

pub struct DeleteProjectResult {
    pub project: Project,
    pub cascaded_tasks_count: usize,
}

pub fn delete_project(
    store: &mut Store,
    storage: &impl Storage,
    parameters: DeleteProjectParameters,
) -> Result<DeleteProjectResult, DeleteProjectError> {
    // Fuzzy match to find project
    let matching_projects: Vec<_> = store
        .get_active_projects()
        .filter(|p| {
            p.name
                .to_lowercase()
                .contains(&parameters.name.to_lowercase())
        })
        .collect();

    let project = match matching_projects.len() {
        0 => return Err(DeleteProjectError::ProjectNotFound(parameters.name)),
        1 => matching_projects[0],
        _ => {
            let names: Vec<String> = matching_projects.iter().map(|p| p.name.clone()).collect();
            return Err(DeleteProjectError::AmbiguousProjectName(names));
        }
    };

    let project_id = project.id;
    let now = jiff::Timestamp::now();

    // Cascade delete: Find all tasks in this project and mark them deleted
    let task_ids_to_delete: Vec<Uuid> = store
        .get_tasks_for_project(project_id)
        .filter(|t| t.deleted_at.is_none())
        .map(|t| t.id)
        .collect();

    let cascade_count = task_ids_to_delete.len();

    for task_id in task_ids_to_delete {
        if let Some(task) = store.get_task_mut(task_id) {
            task.deleted_at = Some(now);
        }
    }

    // Mark project as deleted
    if let Some(project) = store.get_project_mut(project_id) {
        project.deleted_at = Some(now);
    }

    // Persist to storage
    storage.save(store)?;

    Ok(DeleteProjectResult {
        project: store.get_project(project_id).unwrap().clone(),
        cascaded_tasks_count: cascade_count,
    })
}

#[derive(Debug, Error)]
pub enum RestoreProjectError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project '{0}' is not deleted")]
    ProjectNotDeleted(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct RestoreProjectParameters {
    pub name: String,
}

pub fn restore_project(
    store: &mut Store,
    storage: &impl Storage,
    parameters: RestoreProjectParameters,
) -> Result<Project, RestoreProjectError> {
    // Find deleted project by name
    let matching_projects: Vec<_> = store
        .get_deleted_projects()
        .filter(|p| {
            p.name
                .to_lowercase()
                .contains(&parameters.name.to_lowercase())
        })
        .collect();

    let project = match matching_projects.len() {
        0 => return Err(RestoreProjectError::ProjectNotFound(parameters.name)),
        1 => matching_projects[0],
        _ => return Err(RestoreProjectError::ProjectNotFound(parameters.name)),
    };

    let project_id = project.id;

    // Restore project (does NOT auto-restore tasks - user must restore them separately)
    if let Some(project) = store.get_project_mut(project_id) {
        project.deleted_at = None;
    }

    // Persist to storage
    storage.save(store)?;

    Ok(store.get_project(project_id).unwrap().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{area::Area, task::Task};
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
    fn create_test_area(store: &mut Store, name: &str) -> Area {
        let area = Area {
            id: Uuid::new_v4(),
            name: name.to_string(),
            deleted_at: None,
        };
        store.add_area(area.clone());
        area
    }

    fn create_test_task(store: &mut Store, title: &str, project_id: Option<Uuid>) -> Task {
        let task = Task {
            id: Uuid::new_v4(),
            task_number: 0,
            title: title.to_string(),
            project_id,
            created_at: jiff::Timestamp::now(),
            ..Task::default()
        };
        store.add_task(task.clone());
        store
            .get_task_by_number(store.next_task_number - 1)
            .unwrap()
            .clone()
    }

    // ============================================================================
    // create_project tests
    // ============================================================================

    #[test]
    fn test_create_project_without_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Test Project".to_string(),
                area: None,
            },
        );

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.area_id, None);
        assert_eq!(storage.save_count(), 1);
    }

    #[test]
    fn test_create_project_with_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let area = create_test_area(&mut store, "Work");

        let result = create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Work Project".to_string(),
                area: Some(area.name.clone()),
            },
        );

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.area_id, Some(area.id));
    }

    #[test]
    fn test_create_project_area_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Project".to_string(),
                area: Some("non-existent".to_string()),
            },
        );

        assert!(matches!(result, Err(CreateProjectError::AreaNotFound(_))));
    }

    // ============================================================================
    // delete_project tests
    // ============================================================================

    #[test]
    fn test_delete_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Test Project".to_string(),
                area: None,
            },
        )
        .unwrap();

        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Test".to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert!(deleted.project.deleted_at.is_some());
        assert_eq!(deleted.cascaded_tasks_count, 0);
    }

    #[test]
    fn test_delete_project_cascades_to_tasks() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Test Project".to_string(),
                area: None,
            },
        )
        .unwrap();

        // Create tasks in this project
        create_test_task(&mut store, "Task 1", Some(project.id));
        create_test_task(&mut store, "Task 2", Some(project.id));
        create_test_task(&mut store, "Task 3", Some(project.id));

        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Test".to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted.cascaded_tasks_count, 3);

        // Verify tasks are deleted
        let active_tasks: Vec<_> = store.get_active_tasks().collect();
        assert_eq!(active_tasks.len(), 0);
    }

    #[test]
    fn test_delete_project_returns_counts() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Project With Tasks".to_string(),
                area: None,
            },
        )
        .unwrap();

        create_test_task(&mut store, "Task 1", Some(project.id));
        create_test_task(&mut store, "Task 2", Some(project.id));

        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Project".to_string(),
            },
        )
        .unwrap();

        assert_eq!(result.cascaded_tasks_count, 2);
    }

    #[test]
    fn test_delete_project_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "NonExistent".to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(DeleteProjectError::ProjectNotFound(_))
        ));
    }

    #[test]
    fn test_delete_project_ambiguous() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Project One".to_string(),
                area: None,
            },
        )
        .unwrap();

        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Project Two".to_string(),
                area: None,
            },
        )
        .unwrap();

        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Project".to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(DeleteProjectError::AmbiguousProjectName(_))
        ));
    }

    #[test]
    fn test_delete_project_already_deleted() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Test Project".to_string(),
                area: None,
            },
        )
        .unwrap();

        // Delete once
        delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Test".to_string(),
            },
        )
        .unwrap();

        // Try to delete again - should not find it (it's not in active projects)
        let result = delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Test".to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(DeleteProjectError::ProjectNotFound(_))
        ));
    }

    // ============================================================================
    // restore_project tests
    // ============================================================================

    #[test]
    fn test_restore_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Test Project".to_string(),
                area: None,
            },
        )
        .unwrap();

        // Delete first
        delete_project(
            &mut store,
            &storage,
            DeleteProjectParameters {
                name: "Test".to_string(),
            },
        )
        .unwrap();

        // Restore
        let result = restore_project(
            &mut store,
            &storage,
            RestoreProjectParameters {
                name: "Test".to_string(),
            },
        );

        assert!(result.is_ok());
        let restored = result.unwrap();
        assert!(restored.deleted_at.is_none());
    }

    #[test]
    fn test_restore_project_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = restore_project(
            &mut store,
            &storage,
            RestoreProjectParameters {
                name: "NonExistent".to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(RestoreProjectError::ProjectNotFound(_))
        ));
    }

    #[test]
    fn test_restore_project_not_deleted() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        create_project(
            &mut store,
            &storage,
            CreateProjectParameters {
                name: "Active Project".to_string(),
                area: None,
            },
        )
        .unwrap();

        // Try to restore an active (non-deleted) project
        let result = restore_project(
            &mut store,
            &storage,
            RestoreProjectParameters {
                name: "Active".to_string(),
            },
        );

        // Should not find it in deleted projects
        assert!(matches!(
            result,
            Err(RestoreProjectError::ProjectNotFound(_))
        ));
    }
}
