use jiff::civil::Date;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{
        store::Store,
        task::{Task, When},
    },
    storage::{Storage, StorageError},
};

#[derive(Debug, Error)]
pub enum AddTaskError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project name is ambiguous. Multiple projects found: {}", .0.join(", "))]
    AmbiguousProjectName(Vec<String>),

    #[error("Area '{0}' not found")]
    AreaNotFound(String),

    #[error("Area name is ambiguous. Multiple areas found: {}", .0.join(", "))]
    AmbiguousAreaName(Vec<String>),

    #[error("Invalid deadline date '{0}': {1}")]
    InvalidDeadline(String, String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct AddTaskParameters {
    pub title: String,
    pub notes: Option<String>,
    pub when: When,
    pub deadline: Option<String>,
    pub project: Option<String>,
    pub area: Option<String>,
    pub tags: Vec<String>,
}

pub fn add_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: AddTaskParameters,
) -> Result<Task, AddTaskError> {
    // 1. Validate and resolve project name to project ID
    let project_id = if let Some(project_name) = parameters.project {
        let matching_projects: Vec<_> = store
            .get_active_projects()
            .filter(|p| p.name.to_lowercase().contains(&project_name.to_lowercase()))
            .collect();

        match matching_projects.len() {
            0 => return Err(AddTaskError::ProjectNotFound(project_name)),
            1 => Some(matching_projects[0].id),
            _ => {
                let names: Vec<String> = matching_projects.iter().map(|p| p.name.clone()).collect();
                return Err(AddTaskError::AmbiguousProjectName(names));
            }
        }
    } else {
        None
    };

    // 2. Validate and resolve area name to area ID
    let area_id = if let Some(area_name) = parameters.area {
        let matching_areas: Vec<_> = store
            .get_active_areas()
            .filter(|a| a.name.to_lowercase().contains(&area_name.to_lowercase()))
            .collect();

        match matching_areas.len() {
            0 => return Err(AddTaskError::AreaNotFound(area_name)),
            1 => Some(matching_areas[0].id),
            _ => {
                let names: Vec<String> = matching_areas.iter().map(|a| a.name.clone()).collect();
                return Err(AddTaskError::AmbiguousAreaName(names));
            }
        }
    } else {
        None
    };

    // 3. Parse deadline if provided
    let deadline = if let Some(deadline_str) = parameters.deadline {
        Some(
            deadline_str
                .parse::<Date>()
                .map_err(|e| AddTaskError::InvalidDeadline(deadline_str.clone(), e.to_string()))?,
        )
    } else {
        None
    };

    // 4. Create the task (task_number will be assigned by store.add_task)
    let task = Task {
        id: Uuid::new_v4(),
        task_number: 0,
        title: parameters.title,
        notes: parameters.notes,
        project_id,
        area_id,
        tags: parameters.tags,
        when: parameters.when,
        deadline,
        defer_until: None,
        checklist: vec![],
        completed_at: None,
        deleted_at: None,
        created_at: jiff::Timestamp::now(),
    };

    let task_id = task.id;

    // 5. Add to store (assigns task_number)
    store.add_task(task);

    // 6. Persist to storage
    storage.save(store)?;

    // 7. Return the created task (with the assigned task_number)
    Ok(store.get_task(task_id).unwrap().clone())
}

#[derive(Debug, Error)]
pub enum MoveTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task name is ambiguous. Multiple tasks found: {}", .0.join(", "))]
    AmbiguousTaskName(Vec<String>),

    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project name is ambiguous. Multiple projects found: {}", .0.join(", "))]
    AmbiguousProjectName(Vec<String>),

    #[error("Area '{0}' not found")]
    AreaNotFound(String),

    #[error("Area name is ambiguous. Multiple areas found: {}", .0.join(", "))]
    AmbiguousAreaName(Vec<String>),

    #[error("Tag '{0}' not found")]
    TagNotFound(String),

    #[error("Tag name is ambiguous. Multiple tags found: {}", .0.join(", "))]
    AmbiguousTagName(Vec<String>),

    #[error("Invalid deadline date '{0}': {1}")]
    InvalidDeadline(String, String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct MoveTaskParameters {
    pub task_number: u64,
    pub notes: Option<String>,
    pub when: When,
    pub deadline: Option<String>,
    pub project: Option<String>,
    pub area: Option<String>,
    pub tags: Vec<String>,
}

pub fn move_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: MoveTaskParameters,
) -> Result<Task, MoveTaskError> {
    let task =
        store
            .get_task_by_number(parameters.task_number)
            .ok_or(MoveTaskError::TaskNotFound(
                parameters.task_number.to_string(),
            ))?;

    let project_id = if let Some(project_name) = parameters.project {
        let matching_projects: Vec<_> = store
            .get_active_projects()
            .filter(|p| p.name.to_lowercase().contains(&project_name.to_lowercase()))
            .collect();

        match matching_projects.len() {
            0 => return Err(MoveTaskError::ProjectNotFound(project_name)),
            1 => Some(matching_projects[0].id),
            _ => {
                let names: Vec<String> = matching_projects.iter().map(|p| p.name.clone()).collect();
                return Err(MoveTaskError::AmbiguousProjectName(names));
            }
        }
    } else {
        None
    };

    let area_id = if let Some(area_name) = parameters.area {
        let matching_areas: Vec<_> = store
            .get_active_areas()
            .filter(|a| a.name.to_lowercase().contains(&area_name.to_lowercase()))
            .collect();

        match matching_areas.len() {
            0 => return Err(MoveTaskError::AreaNotFound(area_name)),
            1 => Some(matching_areas[0].id),
            _ => {
                let names: Vec<String> = matching_areas.iter().map(|a| a.name.clone()).collect();
                return Err(MoveTaskError::AmbiguousAreaName(names));
            }
        }
    } else {
        None
    };

    let deadline = if let Some(deadline_str) = parameters.deadline {
        Some(
            deadline_str
                .parse::<Date>()
                .map_err(|e| MoveTaskError::InvalidDeadline(deadline_str.clone(), e.to_string()))?,
        )
    } else {
        None
    };

    let new_task = Task {
        id: task.id,
        task_number: task.task_number,
        title: task.title.clone(),
        notes: parameters.notes,
        when: parameters.when,
        deadline,
        project_id,
        area_id,
        tags: parameters.tags,
        defer_until: task.defer_until,
        checklist: task.checklist.clone(),
        completed_at: task.completed_at,
        deleted_at: task.deleted_at,
        created_at: task.created_at,
    };

    let task_id = task.id;

    store.update_task(new_task);

    storage.save(store)?;

    Ok(store.get_task(task_id).unwrap().clone())
}

#[derive(Debug, Error)]
pub enum CompleteTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task name is ambiguous. Multiple tasks found: {}", .0.join(", "))]
    AmbiguousTaskName(Vec<String>),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct CompleteTaskParameters {
    pub task_number_or_fuzzy_name: String,
}

pub fn complete_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: CompleteTaskParameters,
) -> Result<Task, CompleteTaskError> {
    // Try to parse as task number first
    let task = if let Ok(task_number) = parameters.task_number_or_fuzzy_name.parse::<u64>() {
        // Look up by task number
        store.get_task_by_number(task_number).ok_or_else(|| {
            CompleteTaskError::TaskNotFound(parameters.task_number_or_fuzzy_name.clone())
        })?
    } else {
        // Fall back to fuzzy matching by title (similar to how projects/areas work)
        let matching_tasks: Vec<_> = store
            .get_active_tasks()
            .filter(|t| t.completed_at.is_none()) // Only match incomplete tasks
            .filter(|t| {
                t.title
                    .to_lowercase()
                    .contains(&parameters.task_number_or_fuzzy_name.to_lowercase())
            })
            .collect();

        match matching_tasks.len() {
            0 => {
                return Err(CompleteTaskError::TaskNotFound(
                    parameters.task_number_or_fuzzy_name,
                ));
            }
            1 => matching_tasks[0],
            _ => {
                let titles: Vec<String> = matching_tasks.iter().map(|t| t.title.clone()).collect();
                return Err(CompleteTaskError::AmbiguousTaskName(titles));
            }
        }
    };

    // Mark task as completed
    let mut updated_task = task.clone();
    updated_task.completed_at = Some(jiff::Timestamp::now());

    // Update in store
    store.tasks.insert(updated_task.id, updated_task.clone());

    // Persist to storage
    storage.save(store)?;

    Ok(updated_task)
}

#[derive(Debug, Error)]
pub enum DeleteTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task '{0}' is already deleted")]
    TaskAlreadyDeleted(String),

    #[error("Task name is ambiguous. Multiple tasks found: {}", .0.join(", "))]
    AmbiguousTaskName(Vec<String>),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct DeleteTaskParameters {
    pub task_number_or_fuzzy_name: String,
}

pub fn delete_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: DeleteTaskParameters,
) -> Result<Task, DeleteTaskError> {
    // Try to parse as task number first
    let task = if let Ok(task_number) = parameters.task_number_or_fuzzy_name.parse::<u64>() {
        store.get_task_by_number(task_number).ok_or_else(|| {
            DeleteTaskError::TaskNotFound(parameters.task_number_or_fuzzy_name.clone())
        })?
    } else {
        // Fuzzy matching by title (only active tasks)
        let matching_tasks: Vec<_> = store
            .get_active_tasks()
            .filter(|t| {
                t.title
                    .to_lowercase()
                    .contains(&parameters.task_number_or_fuzzy_name.to_lowercase())
            })
            .collect();

        match matching_tasks.len() {
            0 => {
                return Err(DeleteTaskError::TaskNotFound(
                    parameters.task_number_or_fuzzy_name,
                ));
            }
            1 => matching_tasks[0],
            _ => {
                let titles: Vec<String> = matching_tasks.iter().map(|t| t.title.clone()).collect();
                return Err(DeleteTaskError::AmbiguousTaskName(titles));
            }
        }
    };

    // Check if already deleted
    if task.deleted_at.is_some() {
        return Err(DeleteTaskError::TaskAlreadyDeleted(task.title.clone()));
    }

    // Mark as deleted
    let task_id = task.id;
    let mut updated_task = task.clone();
    updated_task.deleted_at = Some(jiff::Timestamp::now());

    // Update in store
    store.tasks.insert(task_id, updated_task.clone());

    // Persist to storage
    storage.save(store)?;

    Ok(updated_task)
}

#[derive(Debug, Error)]
pub enum RestoreTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task '{0}' is not deleted")]
    TaskNotDeleted(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct RestoreTaskParameters {
    pub task_number: u64,
}

pub fn restore_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: RestoreTaskParameters,
) -> Result<Task, RestoreTaskError> {
    let task = store
        .get_task_by_number(parameters.task_number)
        .ok_or_else(|| RestoreTaskError::TaskNotFound(parameters.task_number.to_string()))?;

    // Check if deleted
    if task.deleted_at.is_none() {
        return Err(RestoreTaskError::TaskNotDeleted(task.title.clone()));
    }

    // Restore task
    let task_id = task.id;
    let mut restored_task = task.clone();
    restored_task.deleted_at = None;

    // Update in store
    store.tasks.insert(task_id, restored_task.clone());

    // Persist to storage
    storage.save(store)?;

    Ok(restored_task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{area::Area, project::Project};
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
            slug: slug::slugify(name),
            deleted_at: None,
        };
        store.add_area(area.clone());
        area
    }

    fn create_test_project(store: &mut Store, name: &str, area_id: Option<Uuid>) -> Project {
        let project = Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
            slug: slug::slugify(name),
            area_id,
            created_at: jiff::Timestamp::now(),
            ..Project::default()
        };
        store.add_project(project.clone());
        project
    }

    fn create_test_task(store: &mut Store, title: &str) -> Task {
        let task = Task {
            id: Uuid::new_v4(),
            task_number: 0,
            title: title.to_string(),
            created_at: jiff::Timestamp::now(),
            ..Task::default()
        };
        store.add_task(task.clone());
        store.get_task_by_number(store.next_task_number - 1).unwrap().clone()
    }

    // ============================================================================
    // add_task tests
    // ============================================================================

    #[test]
    fn test_add_task_to_inbox() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Test Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.title, "Test Task");
        assert_eq!(task.task_number, 1);
        assert!(matches!(task.when, When::Inbox));
        assert_eq!(storage.save_count(), 1);
    }

    #[test]
    fn test_add_task_with_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_test_project(&mut store, "Test Project", None);

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task with Project".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: Some("Test".to_string()),
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.project_id, Some(project.id));
    }

    #[test]
    fn test_add_task_with_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let area = create_test_area(&mut store, "Work");

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task with Area".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: Some("work".to_string()),
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.area_id, Some(area.id));
    }

    #[test]
    fn test_add_task_with_tags() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Tagged Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec!["urgent".to_string(), "bug".to_string()],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.tags, vec!["urgent", "bug"]);
    }

    #[test]
    fn test_add_task_with_deadline() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task with Deadline".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: Some("2026-03-15".to_string()),
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert!(task.deadline.is_some());
    }

    #[test]
    fn test_add_task_with_notes() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task with Notes".to_string(),
                notes: Some("Important details".to_string()),
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.notes, Some("Important details".to_string()));
    }

    #[test]
    fn test_add_task_project_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: Some("NonExistent".to_string()),
                area: None,
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(AddTaskError::ProjectNotFound(_))));
    }

    #[test]
    fn test_add_task_ambiguous_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_test_project(&mut store, "Project One", None);
        create_test_project(&mut store, "Project Two", None);

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: Some("Project".to_string()),
                area: None,
                tags: vec![],
            },
        );

        assert!(matches!(
            result,
            Err(AddTaskError::AmbiguousProjectName(_))
        ));
    }

    #[test]
    fn test_add_task_area_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: Some("NonExistent".to_string()),
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(AddTaskError::AreaNotFound(_))));
    }

    #[test]
    fn test_add_task_ambiguous_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_test_area(&mut store, "Area One");
        create_test_area(&mut store, "Area Two");

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: Some("Area".to_string()),
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(AddTaskError::AmbiguousAreaName(_))));
    }

    #[test]
    fn test_add_task_invalid_deadline() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Task".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: Some("invalid-date".to_string()),
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(AddTaskError::InvalidDeadline(_, _))));
    }

    #[test]
    fn test_add_task_assigns_task_number() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let task1 = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "First".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        )
        .unwrap();

        let task2 = add_task(
            &mut store,
            &storage,
            AddTaskParameters {
                title: "Second".to_string(),
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        )
        .unwrap();

        assert_eq!(task1.task_number, 1);
        assert_eq!(task2.task_number, 2);
    }

    // ============================================================================
    // move_task tests
    // ============================================================================

    #[test]
    fn test_move_task_to_today() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Today { evening: false },
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        let moved = result.unwrap();
        assert!(matches!(moved.when, When::Today { evening: false }));
    }

    #[test]
    fn test_move_task_to_someday() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Someday,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Someday));
    }

    #[test]
    fn test_move_task_to_anytime() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Anytime,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Anytime));
    }

    #[test]
    fn test_move_task_with_specific_date() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");
        let date = "2026-04-01".parse().unwrap();

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Scheduled { date },
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Scheduled { .. }));
    }

    #[test]
    fn test_move_task_to_evening() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Today { evening: true },
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Today { evening: true }));
    }

    #[test]
    fn test_move_task_update_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_test_project(&mut store, "New Project", None);
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: Some("New".to_string()),
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().project_id, Some(project.id));
    }

    #[test]
    fn test_move_task_update_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let area = create_test_area(&mut store, "Personal");
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: Some("personal".to_string()),
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().area_id, Some(area.id));
    }

    #[test]
    fn test_move_task_update_notes() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: Some("Updated notes".to_string()),
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().notes, Some("Updated notes".to_string()));
    }

    #[test]
    fn test_move_task_update_tags() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec!["new-tag".to_string()],
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().tags, vec!["new-tag"]);
    }

    #[test]
    fn test_move_task_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: 999,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: None,
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(MoveTaskError::TaskNotFound(_))));
    }

    #[test]
    fn test_move_task_project_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: Some("NonExistent".to_string()),
                area: None,
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(MoveTaskError::ProjectNotFound(_))));
    }

    #[test]
    fn test_move_task_area_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: When::Inbox,
                deadline: None,
                project: None,
                area: Some("NonExistent".to_string()),
                tags: vec![],
            },
        );

        assert!(matches!(result, Err(MoveTaskError::AreaNotFound(_))));
    }

    // ============================================================================
    // complete_task tests
    // ============================================================================

    #[test]
    fn test_complete_task_by_number() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap();
        assert!(completed.completed_at.is_some());
    }

    #[test]
    fn test_complete_task_by_fuzzy_name() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_test_task(&mut store, "Unique Task Name");

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: "unique".to_string(),
            },
        );

        assert!(result.is_ok());
        assert!(result.unwrap().completed_at.is_some());
    }

    #[test]
    fn test_complete_task_sets_timestamp() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");
        let before = jiff::Timestamp::now();

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap();
        assert!(completed.completed_at.is_some());
        assert!(completed.completed_at.unwrap() >= before);
    }

    #[test]
    fn test_complete_task_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: "999".to_string(),
            },
        );

        assert!(matches!(result, Err(CompleteTaskError::TaskNotFound(_))));
    }

    #[test]
    fn test_complete_task_ambiguous_name() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_test_task(&mut store, "Task One");
        create_test_task(&mut store, "Task Two");

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: "Task".to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(CompleteTaskError::AmbiguousTaskName(_))
        ));
    }

    // ============================================================================
    // delete_task tests
    // ============================================================================

    #[test]
    fn test_delete_task_by_number() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert!(deleted.deleted_at.is_some());
    }

    #[test]
    fn test_delete_task_by_fuzzy_name() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        create_test_task(&mut store, "Unique Delete Task");

        let result = delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: "delete".to_string(),
            },
        );

        assert!(result.is_ok());
        assert!(result.unwrap().deleted_at.is_some());
    }

    #[test]
    fn test_delete_task_sets_timestamp() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");
        let before = jiff::Timestamp::now();

        let result = delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        );

        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(deleted.deleted_at.unwrap() >= before);
    }

    #[test]
    fn test_delete_task_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: "999".to_string(),
            },
        );

        assert!(matches!(result, Err(DeleteTaskError::TaskNotFound(_))));
    }

    #[test]
    fn test_delete_task_already_deleted() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        // Delete once
        delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        )
        .unwrap();

        // Try to delete again
        let result = delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        );

        assert!(matches!(
            result,
            Err(DeleteTaskError::TaskAlreadyDeleted(_))
        ));
    }

    // ============================================================================
    // restore_task tests
    // ============================================================================

    #[test]
    fn test_restore_task() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        // Delete first
        delete_task(
            &mut store,
            &storage,
            DeleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
            },
        )
        .unwrap();

        // Restore
        let result = restore_task(
            &mut store,
            &storage,
            RestoreTaskParameters {
                task_number: task.task_number,
            },
        );

        assert!(result.is_ok());
        let restored = result.unwrap();
        assert!(restored.deleted_at.is_none());
    }

    #[test]
    fn test_restore_task_not_found() {
        let storage = MockStorage::new();
        let mut store = Store::default();

        let result = restore_task(
            &mut store,
            &storage,
            RestoreTaskParameters { task_number: 999 },
        );

        assert!(matches!(result, Err(RestoreTaskError::TaskNotFound(_))));
    }

    #[test]
    fn test_restore_task_not_deleted() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = restore_task(
            &mut store,
            &storage,
            RestoreTaskParameters {
                task_number: task.task_number,
            },
        );

        assert!(matches!(result, Err(RestoreTaskError::TaskNotDeleted(_))));
    }
}
