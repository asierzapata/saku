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
