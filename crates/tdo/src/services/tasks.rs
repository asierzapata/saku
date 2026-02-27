use jiff::civil::Date;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{
        store::Store,
        task::{Recurrence, Task, When},
    },
    storage::{Storage, StorageError},
};

#[cfg(feature = "logging")]
use tracing::{error, info, instrument};

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

    #[error("Parent task #{0} not found")]
    ParentTaskNotFound(u64),

    #[error("Cannot create a subtask of a subtask (only one level of nesting is allowed)")]
    ParentIsSubtask,

    #[error("Subtasks inherit project and area from their parent; --project and --area cannot be used with --parent")]
    SubtaskCannotHaveProjectOrArea,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct AddTaskParameters {
    pub title: String,
    pub notes: Option<String>,
    pub when: When,
    pub deadline: Option<String>,
    pub defer_until: Option<String>,
    pub project: Option<String>,
    pub area: Option<String>,
    pub tags: Vec<String>,
    pub recurrence: Option<Recurrence>,
    /// If set, this task becomes a subtask of the given task number
    pub parent_task_number: Option<u64>,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number, task.uuid)))]
pub fn add_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: AddTaskParameters,
) -> Result<Task, AddTaskError> {
    #[cfg(feature = "logging")]
    info!(title = %parameters.title, when = ?parameters.when, "Adding new task");

    // 1. If parent_task_number is provided, resolve parent and inherit context
    let (project_id, area_id, parent_task_id) =
        if let Some(parent_number) = parameters.parent_task_number {
            // Validate no project/area specified alongside --parent
            if parameters.project.is_some() || parameters.area.is_some() {
                return Err(AddTaskError::SubtaskCannotHaveProjectOrArea);
            }

            let parent = store
                .get_task_by_number(parent_number)
                .ok_or(AddTaskError::ParentTaskNotFound(parent_number))?;

            // Enforce one-level nesting
            if parent.parent_task_id.is_some() {
                return Err(AddTaskError::ParentIsSubtask);
            }

            let inherited_project_id = parent.project_id;
            let inherited_area_id = parent.area_id;
            let parent_id = parent.id;
            (inherited_project_id, inherited_area_id, Some(parent_id))
        } else {
            // 1a. Validate and resolve project name to project ID
            let project_id = if let Some(project_name) = parameters.project {
                let matching_projects: Vec<_> = store
                    .get_active_projects()
                    .filter(|p| p.name.to_lowercase().contains(&project_name.to_lowercase()))
                    .collect();

                match matching_projects.len() {
                    0 => {
                        #[cfg(feature = "logging")]
                        error!(project = %project_name, "Project not found");
                        return Err(AddTaskError::ProjectNotFound(project_name));
                    }
                    1 => Some(matching_projects[0].id),
                    _ => {
                        let names: Vec<String> =
                            matching_projects.iter().map(|p| p.name.clone()).collect();
                        #[cfg(feature = "logging")]
                        error!(project = %project_name, matching = ?names, "Ambiguous project name");
                        return Err(AddTaskError::AmbiguousProjectName(names));
                    }
                }
            } else {
                None
            };

            // 1b. Validate and resolve area name to area ID
            let area_id = if let Some(area_name) = parameters.area {
                let matching_areas: Vec<_> = store
                    .get_active_areas()
                    .filter(|a| a.name.to_lowercase().contains(&area_name.to_lowercase()))
                    .collect();

                match matching_areas.len() {
                    0 => {
                        #[cfg(feature = "logging")]
                        error!(area = %area_name, "Area not found");
                        return Err(AddTaskError::AreaNotFound(area_name));
                    }
                    1 => Some(matching_areas[0].id),
                    _ => {
                        let names: Vec<String> =
                            matching_areas.iter().map(|a| a.name.clone()).collect();
                        #[cfg(feature = "logging")]
                        error!(area = %area_name, matching = ?names, "Ambiguous area name");
                        return Err(AddTaskError::AmbiguousAreaName(names));
                    }
                }
            } else {
                None
            };

            (project_id, area_id, None)
        };

    // 3. Parse deadline if provided
    let deadline = if let Some(deadline_str) = parameters.deadline {
        use crate::date_parser::parse_natural_date;
        Some(parse_natural_date(&deadline_str).map_err(|e| {
            #[cfg(feature = "logging")]
            error!(deadline = %deadline_str, error = %e, "Invalid deadline date");
            AddTaskError::InvalidDeadline(deadline_str.clone(), e.to_string())
        })?)
    } else {
        None
    };

    // 3b. Parse defer_until if provided
    let defer_until = if let Some(defer_str) = parameters.defer_until {
        use crate::date_parser::parse_natural_date;
        Some(parse_natural_date(&defer_str).map_err(|e| {
            AddTaskError::InvalidDeadline(defer_str.clone(), e.to_string())
        })?)
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
        defer_until,
        depends_on: vec![],
        parent_task_id,
        completed_at: None,
        deleted_at: None,
        created_at: jiff::Timestamp::now(),
        modified_at: crate::sync_clock::next_modified_at(),
        recurrence: parameters.recurrence,
        completed_occurrences: vec![],
    };

    let task_id = task.id;

    // 5. Add to store (assigns task_number)
    store.add_task(task);

    // 6. Persist to storage
    storage.save(store)?;

    // 7. Return the created task (with the assigned task_number)
    let created_task = store.get_task(task_id).unwrap().clone();

    #[cfg(feature = "logging")]
    {
        let span = tracing::Span::current();
        span.record("task.number", created_task.task_number);
        span.record("task.uuid", created_task.id.to_string());
        info!(
            task_number = created_task.task_number,
            "Task added successfully"
        );
    }

    Ok(created_task)
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

#[derive(Debug)]
pub struct MoveTaskParameters {
    pub task_number: u64,
    pub notes: Option<String>,
    pub when: Option<When>,
    pub deadline: Option<String>,
    pub clear_schedule: bool,
    pub clear_deadline: bool,
    pub project: Option<String>,
    pub area: Option<String>,
    /// Remove the project assignment entirely.
    pub clear_project: bool,
    /// Remove the area assignment entirely.
    pub clear_area: bool,
    pub tags: Vec<String>,
    /// Set a new defer-until date.
    pub defer_until: Option<String>,
    /// Remove the defer-until date entirely.
    pub clear_defer: bool,
    /// Set a new recurrence rule.
    pub recurrence: Option<Recurrence>,
    /// Remove the recurrence rule entirely.
    pub clear_recurrence: bool,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number)))]
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

    let existing_project_id = task.project_id;
    let project_id = if parameters.clear_project {
        None
    } else if let Some(project_name) = parameters.project {
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
        existing_project_id
    };

    let existing_area_id = task.area_id;
    let area_id = if parameters.clear_area {
        None
    } else if let Some(area_name) = parameters.area {
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
        existing_area_id
    };

    let deadline = if parameters.clear_deadline {
        None
    } else if let Some(deadline_str) = parameters.deadline {
        use crate::date_parser::parse_natural_date;
        Some(
            parse_natural_date(&deadline_str)
                .map_err(|e| MoveTaskError::InvalidDeadline(deadline_str.clone(), e.to_string()))?,
        )
    } else {
        task.deadline
    };

    let when = if parameters.clear_schedule {
        // Clear schedule - go to inbox if no deadline, otherwise keep current
        if deadline.is_some() {
            task.when.clone()
        } else {
            When::Inbox
        }
    } else if let Some(new_when) = parameters.when {
        new_when
    } else {
        task.when.clone()
    };

    let defer_until = if parameters.clear_defer {
        None
    } else if let Some(defer_str) = parameters.defer_until {
        use crate::date_parser::parse_natural_date;
        Some(
            parse_natural_date(&defer_str)
                .map_err(|e| MoveTaskError::InvalidDeadline(defer_str.clone(), e.to_string()))?,
        )
    } else {
        task.defer_until
    };

    let recurrence = if parameters.clear_recurrence {
        None
    } else if let Some(r) = parameters.recurrence {
        Some(r)
    } else {
        task.recurrence.clone()
    };

    let new_task = Task {
        id: task.id,
        task_number: task.task_number,
        title: task.title.clone(),
        notes: parameters.notes,
        when,
        deadline,
        project_id,
        area_id,
        tags: parameters.tags,
        defer_until,
        depends_on: task.depends_on.clone(),
        parent_task_id: task.parent_task_id,
        completed_at: task.completed_at,
        deleted_at: task.deleted_at,
        created_at: task.created_at,
        modified_at: crate::sync_clock::next_modified_at(),
        recurrence,
        completed_occurrences: task.completed_occurrences.clone(),
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

    #[error("Cannot complete task: it has incomplete subtasks")]
    HasIncompleteSubtasks,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct CompleteTaskParameters {
    pub task_number_or_fuzzy_name: String,
    /// If true and the task is recurring, cancel it permanently (sets completed_at).
    /// For non-recurring tasks this flag is ignored and completed_at is always set.
    pub stop: bool,
    /// Optional note to append to the task when completing it.
    pub note: Option<String>,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number)))]
pub fn complete_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: CompleteTaskParameters,
) -> Result<CompleteTaskResult, CompleteTaskError> {
    #[cfg(feature = "logging")]
    info!(identifier = %parameters.task_number_or_fuzzy_name, "Completing task");

    // Try to parse as task number first
    let task = if let Ok(task_number) = parameters.task_number_or_fuzzy_name.parse::<u64>() {
        // Look up by task number
        store.get_task_by_number(task_number).ok_or_else(|| {
            #[cfg(feature = "logging")]
            error!(task_number = task_number, "Task not found");
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
                #[cfg(feature = "logging")]
                error!(identifier = %parameters.task_number_or_fuzzy_name, "Task not found");
                return Err(CompleteTaskError::TaskNotFound(
                    parameters.task_number_or_fuzzy_name,
                ));
            }
            1 => matching_tasks[0],
            _ => {
                let titles: Vec<String> = matching_tasks.iter().map(|t| t.title.clone()).collect();
                #[cfg(feature = "logging")]
                error!(identifier = %parameters.task_number_or_fuzzy_name, matching = ?titles, "Ambiguous task name");
                return Err(CompleteTaskError::AmbiguousTaskName(titles));
            }
        }
    };

    #[cfg(feature = "logging")]
    tracing::Span::current().record("task.number", task.task_number);

    let task_id = task.id;

    // Prevent completing a task that has incomplete subtasks
    if store.has_incomplete_subtasks(task_id) {
        return Err(CompleteTaskError::HasIncompleteSubtasks);
    }

    // Collect tasks that were blocked by this task before completing it
    let previously_blocking: Vec<Task> = store
        .get_blocking(task_id)
        .into_iter()
        .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
        .cloned()
        .collect();

    // Mark task as completed — behaviour differs for recurring tasks.
    let mut updated_task = task.clone();
    updated_task.modified_at = crate::sync_clock::next_modified_at();

    // Append note if provided
    if let Some(note_text) = &parameters.note {
        updated_task.notes = Some(match &updated_task.notes {
            Some(existing) => format!("{}\n{}", existing, note_text),
            None => note_text.clone(),
        });
    }

    if task.recurrence.is_some() && !parameters.stop {
        // Recurring task: record today as the completed occurrence, do not set completed_at.
        let occurrence_date = jiff::Zoned::now().date();
        if !updated_task.completed_occurrences.contains(&occurrence_date) {
            updated_task.completed_occurrences.push(occurrence_date);
        }
    } else {
        // Non-recurring or stop=true: mark the task as permanently done.
        updated_task.completed_at = Some(jiff::Timestamp::now());
    }

    // Update in store
    store.tasks.insert(updated_task.id, updated_task.clone());

    // Persist to storage
    storage.save(store)?;

    #[cfg(feature = "logging")]
    info!(
        task_number = updated_task.task_number,
        "Task completed successfully"
    );

    // Determine which previously-blocked tasks are now fully unblocked
    let newly_unblocked: Vec<Task> = previously_blocking
        .into_iter()
        .filter(|t| !store.is_task_blocked(t))
        .map(|t| store.get_task(t.id).unwrap().clone())
        .collect();

    Ok(CompleteTaskResult {
        task: updated_task,
        newly_unblocked,
    })
}

/// Result returned by complete_task, including any tasks that became unblocked.
pub struct CompleteTaskResult {
    pub task: Task,
    pub newly_unblocked: Vec<Task>,
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

#[derive(Debug)]
pub struct DeleteTaskParameters {
    pub task_number_or_fuzzy_name: String,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number)))]
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
    updated_task.modified_at = crate::sync_clock::next_modified_at();

    // Update in store
    store.tasks.insert(task_id, updated_task.clone());

    // Cascade soft-delete all non-deleted subtasks
    let subtask_ids: Vec<uuid::Uuid> = store
        .get_subtasks(task_id)
        .map(|t| t.id)
        .collect();
    let now = jiff::Timestamp::now();
    for subtask_id in subtask_ids {
        if let Some(subtask) = store.get_task_mut(subtask_id) {
            subtask.deleted_at = Some(now);
            subtask.modified_at = crate::sync_clock::next_modified_at();
        }
    }

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

#[derive(Debug)]
pub struct RestoreTaskParameters {
    pub task_number: u64,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number = parameters.task_number)))]
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
    restored_task.modified_at = crate::sync_clock::next_modified_at();

    // Update in store
    store.tasks.insert(task_id, restored_task.clone());

    // Persist to storage
    storage.save(store)?;

    Ok(restored_task)
}

#[derive(Debug, Error)]
pub enum EditTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task name is ambiguous. Multiple tasks found: {}", .0.join(", "))]
    AmbiguousTaskName(Vec<String>),

    #[error("Failed to open editor: {0}")]
    EditorFailed(String),

    #[error("Failed to parse edited task: {0}")]
    ParseFailed(String),

    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project name is ambiguous. Multiple projects found: {}", .0.join(", "))]
    AmbiguousProjectName(Vec<String>),

    #[error("Area '{0}' not found")]
    AreaNotFound(String),

    #[error("Area name is ambiguous. Multiple areas found: {}", .0.join(", "))]
    AmbiguousAreaName(Vec<String>),

    #[error("Invalid date format for '{0}': {1}")]
    InvalidDate(String, String),

    #[error(
        "Invalid 'when' value: {0}. Expected: inbox, today, today-evening, anytime, someday, or YYYY-MM-DD"
    )]
    InvalidWhen(String),

    #[error("No changes detected")]
    NoChanges,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct EditTaskParameters {
    pub task_number_or_fuzzy_name: String,
}

#[cfg_attr(feature = "logging", instrument(skip(store, storage), fields(task.number)))]
pub fn edit_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: EditTaskParameters,
) -> Result<Task, EditTaskError> {
    use crate::services::task_editor;

    // 1. Find task by number or fuzzy name match
    let task = if let Ok(task_number) = parameters.task_number_or_fuzzy_name.parse::<u64>() {
        store.get_task_by_number(task_number).ok_or_else(|| {
            EditTaskError::TaskNotFound(parameters.task_number_or_fuzzy_name.clone())
        })?
    } else {
        // Fuzzy matching by title
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
                return Err(EditTaskError::TaskNotFound(
                    parameters.task_number_or_fuzzy_name,
                ));
            }
            1 => matching_tasks[0],
            _ => {
                let titles: Vec<String> = matching_tasks.iter().map(|t| t.title.clone()).collect();
                return Err(EditTaskError::AmbiguousTaskName(titles));
            }
        }
    };

    // 2. Serialize task to editor format
    let editor_content = task_editor::serialize_task_for_edit(task, store);

    // 3. Open editor and get modified content
    let modified_content =
        task_editor::open_in_editor(&editor_content).map_err(EditTaskError::EditorFailed)?;

    // 4. Parse edited content
    let parsed =
        task_editor::parse_edited_task(&modified_content).map_err(EditTaskError::ParseFailed)?;

    // 5. Validate changes detected
    if !task_editor::has_changes(task, &parsed, store) {
        return Err(EditTaskError::NoChanges);
    }

    // 6. Validate and resolve project name to ID
    let project_id = if let Some(project_name) = parsed.project {
        let matching_projects: Vec<_> = store
            .get_active_projects()
            .filter(|p| p.name.to_lowercase().contains(&project_name.to_lowercase()))
            .collect();

        match matching_projects.len() {
            0 => return Err(EditTaskError::ProjectNotFound(project_name)),
            1 => Some(matching_projects[0].id),
            _ => {
                let names: Vec<String> = matching_projects.iter().map(|p| p.name.clone()).collect();
                return Err(EditTaskError::AmbiguousProjectName(names));
            }
        }
    } else {
        None
    };

    // 7. Validate and resolve area name to ID
    let area_id = if let Some(area_name) = parsed.area {
        let matching_areas: Vec<_> = store
            .get_active_areas()
            .filter(|a| a.name.to_lowercase().contains(&area_name.to_lowercase()))
            .collect();

        match matching_areas.len() {
            0 => return Err(EditTaskError::AreaNotFound(area_name)),
            1 => Some(matching_areas[0].id),
            _ => {
                let names: Vec<String> = matching_areas.iter().map(|a| a.name.clone()).collect();
                return Err(EditTaskError::AmbiguousAreaName(names));
            }
        }
    } else {
        None
    };

    // 8. Parse when string
    let when = parse_when_string(&parsed.when)
        .map_err(|_| EditTaskError::InvalidWhen(parsed.when.clone()))?;

    // 9. Parse deadline if provided
    let deadline = if let Some(deadline_str) = parsed.deadline {
        Some(
            deadline_str
                .parse::<Date>()
                .map_err(|e| EditTaskError::InvalidDate("deadline".to_string(), e.to_string()))?,
        )
    } else {
        None
    };

    // 10. Parse defer_until if provided
    let defer_until =
        if let Some(defer_str) = parsed.defer_until {
            Some(defer_str.parse::<Date>().map_err(|e| {
                EditTaskError::InvalidDate("defer_until".to_string(), e.to_string())
            })?)
        } else {
            None
        };

    // 11. Build updated task
    let updated_task = Task {
        id: task.id,
        task_number: task.task_number,
        title: parsed.title,
        notes: parsed.notes,
        project_id,
        area_id,
        tags: parsed.tags,
        when,
        deadline,
        defer_until,
        depends_on: task.depends_on.clone(),
        parent_task_id: task.parent_task_id,
        completed_at: task.completed_at,
        deleted_at: task.deleted_at,
        created_at: task.created_at,
        modified_at: crate::sync_clock::next_modified_at(),
        recurrence: task.recurrence.clone(),
        completed_occurrences: task.completed_occurrences.clone(),
    };

    let task_id = task.id;

    // 13. Update store
    store.update_task(updated_task);

    // 14. Persist to storage
    storage.save(store)?;

    // 15. Return updated task
    Ok(store.get_task(task_id).unwrap().clone())
}

/// Parse when string to When enum
fn parse_when_string(s: &str) -> Result<When, ()> {
    use crate::date_parser::parse_natural_date;
    use jiff::Zoned;

    match s.to_lowercase().as_str() {
        "inbox" => Ok(When::Inbox),
        "today" => {
            let today_date = Zoned::now().date();
            Ok(When::Scheduled { date: today_date })
        }
        "someday" => Ok(When::Someday),
        _ => {
            // Try to parse as natural language date
            parse_natural_date(s)
                .map(|date| When::Scheduled { date })
                .map_err(|_| ())
        }
    }
}

// ============================================================================
// depend_task
// ============================================================================

#[derive(Debug, Error)]
pub enum DependTaskError {
    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("A task cannot depend on itself")]
    SelfDependency,

    #[error("This would create a circular dependency")]
    CircularDependency,

    #[error("Dependency already exists")]
    AlreadyExists,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct DependTaskParameters {
    /// The task that gets the dependency (the blocked task)
    pub task_number: u64,
    /// Task number to add as a dependency (None = remove mode)
    pub add_dependency: Option<u64>,
    /// Task number to remove as a dependency
    pub remove_dependency: Option<u64>,
}

/// Check for circular dependencies transitively: would adding `new_dep_id` as a
/// dependency of `task_id` create a cycle?
fn would_create_cycle(store: &Store, task_id: Uuid, new_dep_id: Uuid) -> bool {
    // BFS/DFS: starting from new_dep_id, see if we can reach task_id through depends_on chains
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(new_dep_id);

    while let Some(current) = queue.pop_front() {
        if current == task_id {
            return true;
        }
        if !visited.insert(current) {
            continue;
        }
        if let Some(t) = store.tasks.get(&current) {
            for dep in &t.depends_on {
                queue.push_back(*dep);
            }
        }
    }
    false
}

pub fn depend_task(
    store: &mut Store,
    storage: &impl Storage,
    parameters: DependTaskParameters,
) -> Result<Task, DependTaskError> {
    let task = store
        .get_task_by_number(parameters.task_number)
        .ok_or_else(|| DependTaskError::TaskNotFound(parameters.task_number.to_string()))?
        .clone();

    let mut updated = task.clone();

    if let Some(dep_number) = parameters.add_dependency {
        let dep = store
            .get_task_by_number(dep_number)
            .ok_or_else(|| DependTaskError::TaskNotFound(dep_number.to_string()))?
            .clone();

        if dep.id == task.id {
            return Err(DependTaskError::SelfDependency);
        }

        if updated.depends_on.contains(&dep.id) {
            return Err(DependTaskError::AlreadyExists);
        }

        if would_create_cycle(store, task.id, dep.id) {
            return Err(DependTaskError::CircularDependency);
        }

        updated.depends_on.push(dep.id);
    }

    if let Some(rem_number) = parameters.remove_dependency {
        let dep = store
            .get_task_by_number(rem_number)
            .ok_or_else(|| DependTaskError::TaskNotFound(rem_number.to_string()))?
            .clone();

        updated.depends_on.retain(|id| *id != dep.id);
    }

    updated.modified_at = crate::sync_clock::next_modified_at();
    let task_id = updated.id;
    store.update_task(updated);
    storage.save(store)?;

    Ok(store.get_task(task_id).unwrap().clone())
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
            deleted_at: None,
            modified_at: saku_storage::timestamp::HybridTimestamp::default(),
        };
        store.add_area(area.clone());
        area
    }

    fn create_test_project(store: &mut Store, name: &str, area_id: Option<Uuid>) -> Project {
        let project = Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
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
        store
            .get_task_by_number(store.next_task_number - 1)
            .unwrap()
            .clone()
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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
                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
            },
        );

        assert!(matches!(result, Err(AddTaskError::AmbiguousProjectName(_))));
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

                recurrence: None,
                parent_task_number: None,
                defer_until: None,
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

        let today = jiff::Zoned::now().date();
        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: Some(When::Scheduled { date: today }),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        let moved = result.unwrap();
        assert!(matches!(moved.when, When::Scheduled { date: _ }));
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
                when: Some(When::Someday),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Someday));
    }

    #[test]
    fn test_move_task_to_someday_legacy() {
        // Test that demonstrates Someday behavior (Anytime was replaced with Someday)
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: Some(When::Someday),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Someday));
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
                when: Some(When::Scheduled { date }),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert!(matches!(result.unwrap().when, When::Scheduled { .. }));
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: Some("New".to_string()),
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: Some("personal".to_string()),
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec!["new-tag".to_string()],
                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: Some("NonExistent".to_string()),
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
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
                when: Some(When::Inbox),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: Some("NonExistent".to_string()),
                clear_project: false,
                clear_area: false,
                tags: vec![],

                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(matches!(result, Err(MoveTaskError::AreaNotFound(_))));
    }

    #[test]
    fn test_move_task_preserves_project_when_not_specified() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_test_project(&mut store, "My Project", None);
        let task = {
            let t = Task {
                id: Uuid::new_v4(),
                task_number: 0,
                title: "Task with Project".to_string(),
                project_id: Some(project.id),
                created_at: jiff::Timestamp::now(),
                ..Task::default()
            };
            store.add_task(t);
            store.get_task_by_number(store.next_task_number - 1).unwrap().clone()
        };

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: Some(When::Someday),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],
                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().project_id, Some(project.id));
    }

    #[test]
    fn test_move_task_clear_project() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let project = create_test_project(&mut store, "My Project", None);
        let task = {
            let t = Task {
                id: Uuid::new_v4(),
                task_number: 0,
                title: "Task with Project".to_string(),
                project_id: Some(project.id),
                created_at: jiff::Timestamp::now(),
                ..Task::default()
            };
            store.add_task(t);
            store.get_task_by_number(store.next_task_number - 1).unwrap().clone()
        };

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: None,
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: true,
                clear_area: false,
                tags: vec![],
                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().project_id, None);
    }

    #[test]
    fn test_move_task_preserves_area_when_not_specified() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let area = create_test_area(&mut store, "Work");
        let task = {
            let t = Task {
                id: Uuid::new_v4(),
                task_number: 0,
                title: "Task with Area".to_string(),
                area_id: Some(area.id),
                created_at: jiff::Timestamp::now(),
                ..Task::default()
            };
            store.add_task(t);
            store.get_task_by_number(store.next_task_number - 1).unwrap().clone()
        };

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: Some(When::Someday),
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: false,
                tags: vec![],
                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().area_id, Some(area.id));
    }

    #[test]
    fn test_move_task_clear_area() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let area = create_test_area(&mut store, "Work");
        let task = {
            let t = Task {
                id: Uuid::new_v4(),
                task_number: 0,
                title: "Task with Area".to_string(),
                area_id: Some(area.id),
                created_at: jiff::Timestamp::now(),
                ..Task::default()
            };
            store.add_task(t);
            store.get_task_by_number(store.next_task_number - 1).unwrap().clone()
        };

        let result = move_task(
            &mut store,
            &storage,
            MoveTaskParameters {
                task_number: task.task_number,
                notes: None,
                when: None,
                deadline: None,
                clear_schedule: false,
                clear_deadline: false,
                project: None,
                area: None,
                clear_project: false,
                clear_area: true,
                tags: vec![],
                recurrence: None,
                clear_recurrence: false,
                defer_until: None,
                clear_defer: false,
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().area_id, None);
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
                stop: false,
                note: None,
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap().task;
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
                stop: false,
                note: None,
            },
        );

        assert!(result.is_ok());
        assert!(result.unwrap().task.completed_at.is_some());
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
                stop: false,
                note: None,
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap().task;
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
                stop: false,
                note: None,
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
                stop: false,
                note: None,
            },
        );

        assert!(matches!(
            result,
            Err(CompleteTaskError::AmbiguousTaskName(_))
        ));
    }

    #[test]
    fn test_complete_task_with_note_sets_note() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let task = create_test_task(&mut store, "Test Task");

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
                stop: false,
                note: Some("Completion note".to_string()),
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap().task;
        assert_eq!(completed.notes, Some("Completion note".to_string()));
    }

    #[test]
    fn test_complete_task_with_note_appends_to_existing() {
        let storage = MockStorage::new();
        let mut store = Store::default();
        let mut task = create_test_task(&mut store, "Test Task");
        task.notes = Some("Existing note".to_string());
        store.tasks.insert(task.id, task.clone());

        let result = complete_task(
            &mut store,
            &storage,
            CompleteTaskParameters {
                task_number_or_fuzzy_name: task.task_number.to_string(),
                stop: false,
                note: Some("Completion note".to_string()),
            },
        );

        assert!(result.is_ok());
        let completed = result.unwrap().task;
        assert_eq!(
            completed.notes,
            Some("Existing note\nCompletion note".to_string())
        );
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
