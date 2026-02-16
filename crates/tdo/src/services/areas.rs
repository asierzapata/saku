use crate::{
    models::{area::Area, store::Store},
    storage::{Storage, StorageError},
};
use slug::slugify;
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
    let area_slug = slugify(&parameters.name);

    let area = Area {
        name: parameters.name,
        slug: area_slug,
        ..Area::default()
    };

    let area_id = area.id;

    store.add_area(area);

    storage.save(store)?;

    Ok(store.get_area(area_id).unwrap().clone())
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

    let area_id = area.id;
    let now = jiff::Timestamp::now();

    // Cascade delete: Find all projects in this area
    let project_ids_to_delete: Vec<uuid::Uuid> = store
        .get_projects_for_area(area_id)
        .filter(|p| p.deleted_at.is_none())
        .map(|p| p.id)
        .collect();

    let mut total_tasks_deleted = 0;

    // For each project, cascade delete its tasks
    for project_id in &project_ids_to_delete {
        let task_ids: Vec<uuid::Uuid> = store
            .get_tasks_for_project(*project_id)
            .filter(|t| t.deleted_at.is_none())
            .map(|t| t.id)
            .collect();

        total_tasks_deleted += task_ids.len();

        for task_id in task_ids {
            if let Some(task) = store.get_task_mut(task_id) {
                task.deleted_at = Some(now);
            }
        }
    }

    // Mark all projects in this area as deleted
    for project_id in &project_ids_to_delete {
        if let Some(project) = store.get_project_mut(*project_id) {
            project.deleted_at = Some(now);
        }
    }

    // Also delete tasks directly under this area (not in a project)
    let direct_task_ids: Vec<uuid::Uuid> = store
        .get_tasks_for_area(area_id)
        .filter(|t| t.deleted_at.is_none())
        .map(|t| t.id)
        .collect();

    total_tasks_deleted += direct_task_ids.len();

    for task_id in direct_task_ids {
        if let Some(task) = store.get_task_mut(task_id) {
            task.deleted_at = Some(now);
        }
    }

    // Mark area as deleted
    if let Some(area) = store.get_area_mut(area_id) {
        area.deleted_at = Some(now);
    }

    // Persist to storage
    storage.save(store)?;

    Ok(DeleteAreaResult {
        area: store.get_area(area_id).unwrap().clone(),
        cascaded_projects_count: project_ids_to_delete.len(),
        cascaded_tasks_count: total_tasks_deleted,
    })
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

    let area_id = area.id;

    // Restore area (does NOT auto-restore projects/tasks - user must restore them separately)
    if let Some(area) = store.get_area_mut(area_id) {
        area.deleted_at = None;
    }

    // Persist to storage
    storage.save(store)?;

    Ok(store.get_area(area_id).unwrap().clone())
}
