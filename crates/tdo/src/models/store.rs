use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::{area::Area, project::Project, task::Task};

/// Current schema version
pub const CURRENT_VERSION: u32 = 8;

/// Storage representation (how data lives on disk as JSON)
#[derive(Serialize, Deserialize)]
pub struct StoredStore {
    pub version: u32,
    pub next_task_number: u64,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub areas: Vec<Area>,
}

impl Default for StoredStore {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            next_task_number: 1,
            tasks: vec![],
            projects: vec![],
            areas: vec![],
        }
    }
}

/// In-memory representation (how we work with data in the app)
#[derive(Clone)]
pub struct Store {
    pub version: u32,
    pub next_task_number: u64,
    pub tasks: HashMap<Uuid, Task>,
    pub projects: HashMap<Uuid, Project>,
    pub areas: HashMap<Uuid, Area>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            next_task_number: 1,
            tasks: HashMap::new(),
            projects: HashMap::new(),
            areas: HashMap::new(),
        }
    }
}

impl Store {
    /// Convert from storage format (Vec) to working format (HashMap)
    pub fn from_stored(stored: StoredStore) -> Self {
        let tasks: HashMap<_, _> = stored
            .tasks
            .into_iter()
            .map(|mut t| {
                // Normalize legacy When variants during load
                t.when = t.when.normalize();
                (t.id, t)
            })
            .collect();

        let projects: HashMap<_, _> = stored.projects.into_iter().map(|p| (p.id, p)).collect();

        let areas: HashMap<_, _> = stored.areas.into_iter().map(|a| (a.id, a)).collect();

        Self {
            version: stored.version,
            next_task_number: stored.next_task_number,
            tasks,
            projects,
            areas,
        }
    }

    /// Convert from working format (HashMap) to storage format (Vec)
    pub fn to_stored(&self) -> StoredStore {
        StoredStore {
            version: self.version,
            next_task_number: self.next_task_number,
            tasks: self.tasks.values().cloned().collect(),
            projects: self.projects.values().cloned().collect(),
            areas: self.areas.values().cloned().collect(),
        }
    }

    /// Add a task to the store, assigning it the next task_number
    pub fn add_task(&mut self, mut task: Task) {
        task.task_number = self.next_task_number;
        self.next_task_number += 1;
        self.tasks.insert(task.id, task);
    }

    /// Update an existing task in the store
    pub fn update_task(&mut self, task: Task) {
        self.tasks.insert(task.id, task);
    }

    /// Add a project to the store
    pub fn add_project(&mut self, project: Project) {
        self.projects.insert(project.id, project);
    }

    /// Add an area to the store
    pub fn add_area(&mut self, area: Area) {
        self.areas.insert(area.id, area);
    }

    /// Get a task by ID
    pub fn get_task(&self, id: Uuid) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Look up a task by its user-facing task_number
    pub fn get_task_by_number(&self, number: u64) -> Option<&Task> {
        self.tasks.values().find(|t| t.task_number == number)
    }

    /// Get a project by ID
    pub fn get_project(&self, id: Uuid) -> Option<&Project> {
        self.projects.get(&id)
    }

    /// Get an area by ID
    pub fn get_area(&self, id: Uuid) -> Option<&Area> {
        self.areas.get(&id)
    }

    /// Get all active (non-deleted) tasks
    pub fn get_active_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values().filter(|t| t.deleted_at.is_none())
    }

    /// Get all active (non-deleted) projects
    pub fn get_active_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values().filter(|p| p.deleted_at.is_none())
    }

    /// Get all active (non-deleted) areas
    pub fn get_active_areas(&self) -> impl Iterator<Item = &Area> {
        self.areas.values().filter(|a| a.deleted_at.is_none())
    }

    /// Get all deleted tasks (for trash view)
    pub fn get_deleted_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values().filter(|t| t.deleted_at.is_some())
    }

    /// Get all deleted projects (for trash view)
    pub fn get_deleted_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values().filter(|p| p.deleted_at.is_some())
    }

    /// Get all deleted areas (for trash view)
    pub fn get_deleted_areas(&self) -> impl Iterator<Item = &Area> {
        self.areas.values().filter(|a| a.deleted_at.is_some())
    }

    /// Get a mutable task by ID
    pub fn get_task_mut(&mut self, id: Uuid) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// Get a mutable project by ID
    pub fn get_project_mut(&mut self, id: Uuid) -> Option<&mut Project> {
        self.projects.get_mut(&id)
    }

    /// Get a mutable area by ID
    pub fn get_area_mut(&mut self, id: Uuid) -> Option<&mut Area> {
        self.areas.get_mut(&id)
    }

    /// Find tasks belonging to a project
    pub fn get_tasks_for_project(&self, project_id: Uuid) -> impl Iterator<Item = &Task> {
        self.tasks
            .values()
            .filter(move |t| t.project_id == Some(project_id))
    }

    /// Find projects belonging to an area
    pub fn get_projects_for_area(&self, area_id: Uuid) -> impl Iterator<Item = &Project> {
        self.projects
            .values()
            .filter(move |p| p.area_id == Some(area_id))
    }

    /// Find tasks directly belonging to an area (no project)
    pub fn get_tasks_for_area(&self, area_id: Uuid) -> impl Iterator<Item = &Task> {
        self.tasks
            .values()
            .filter(move |t| t.area_id == Some(area_id) && t.project_id.is_none())
    }

    /// Find non-deleted subtasks of a given parent task
    pub fn get_subtasks(&self, parent_id: Uuid) -> impl Iterator<Item = &Task> {
        self.tasks
            .values()
            .filter(move |t| t.parent_task_id == Some(parent_id) && t.deleted_at.is_none())
    }

    /// Returns true if the task has at least one incomplete, non-deleted subtask.
    pub fn has_incomplete_subtasks(&self, task_id: Uuid) -> bool {
        self.get_subtasks(task_id)
            .any(|t| t.completed_at.is_none())
    }

    /// Returns true if the task has at least one incomplete, non-deleted dependency.
    pub fn is_task_blocked(&self, task: &Task) -> bool {
        task.depends_on.iter().any(|dep_id| {
            self.tasks.get(dep_id).is_some_and(|dep| {
                dep.completed_at.is_none() && dep.deleted_at.is_none()
            })
        })
    }

    /// Returns all tasks that are directly blocking the given task (incomplete, non-deleted).
    pub fn get_blockers(&self, task: &Task) -> Vec<&Task> {
        task.depends_on
            .iter()
            .filter_map(|dep_id| self.tasks.get(dep_id))
            .filter(|dep| dep.completed_at.is_none() && dep.deleted_at.is_none())
            .collect()
    }

    /// Returns all tasks that depend on the given task (tasks that this task is blocking).
    pub fn get_blocking(&self, task_id: Uuid) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|t| t.depends_on.contains(&task_id))
            .collect()
    }

    /// Active tasks that have a recurrence rule and have not been permanently stopped.
    pub fn get_recurring_tasks(&self) -> impl Iterator<Item = &Task> {
        self.get_active_tasks()
            .filter(|t| t.recurrence.is_some() && t.completed_at.is_none())
    }

    /// Search active tasks by case-insensitive substring match on title (and optionally notes).
    pub fn search_tasks(&self, query: &str, include_notes: bool) -> Vec<&Task> {
        let query_lower = query.to_lowercase();
        self.get_active_tasks()
            .filter(|t| {
                t.title.to_lowercase().contains(&query_lower)
                    || (include_notes
                        && t.notes
                            .as_ref()
                            .is_some_and(|n| n.to_lowercase().contains(&query_lower)))
            })
            .collect()
    }
}
