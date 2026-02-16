use crate::{
    models::{project::Project, store::Store},
    storage::{Storage, StorageError},
};
use slug::slugify;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateProjectError {
    #[error("Area with name '{}' not found", .0)]
    AreaNotFound(String),

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
    let project_slug = slugify(&parameters.name);

    let area_id = match parameters.area {
        Some(area_slug) => Some(
            store
                .get_area_by_slug(&area_slug)
                .ok_or(CreateProjectError::AreaNotFound(area_slug))?
                .id,
        ),
        None => None,
    };

    let project = Project {
        id: Uuid::new_v4(),
        name: parameters.name,
        slug: project_slug,
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
