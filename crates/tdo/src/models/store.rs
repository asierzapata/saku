use saku_storage::entity::Entity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::models::{area::Area, project::Project, task::Task};

/// Current schema version
pub const CURRENT_VERSION: u32 = 9;

/// Storage representation (how data lives on disk as JSON).
/// v9: flat KV entries map keyed by storage key.
#[derive(Serialize, Deserialize)]
pub struct StoredStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,
}

impl Default for StoredStore {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: HashMap::new(),
        }
    }
}

/// In-memory representation (how we work with data in the app).
/// Keys are full storage keys like "task/k7m2a3x9", "project/website", "area/work".
#[derive(Clone)]
pub struct Store {
    pub version: u32,
    pub tasks: HashMap<String, Task>,
    pub projects: HashMap<String, Project>,
    pub areas: HashMap<String, Area>,
    /// Secondary index: task_number → storage_key for O(1) lookup
    pub task_number_index: HashMap<u64, String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            tasks: HashMap::new(),
            projects: HashMap::new(),
            areas: HashMap::new(),
            task_number_index: HashMap::new(),
        }
    }
}

impl Store {
    /// Convert from storage format (KV entries) to working format (typed HashMaps).
    pub fn from_stored(stored: StoredStore) -> Self {
        let mut tasks = HashMap::new();
        let mut projects = HashMap::new();
        let mut areas = HashMap::new();
        let mut task_number_index = HashMap::new();

        for (key, value) in stored.entries {
            if key.starts_with("task/") {
                if let Ok(mut task) = serde_json::from_value::<Task>(value) {
                    // Normalize legacy When variants during load
                    task.when = task.when.normalize();
                    task_number_index.insert(task.task_number, key.clone());
                    tasks.insert(key, task);
                }
            } else if key.starts_with("project/") {
                if let Ok(project) = serde_json::from_value::<Project>(value) {
                    projects.insert(key, project);
                }
            } else if key.starts_with("area/") {
                if let Ok(area) = serde_json::from_value::<Area>(value) {
                    areas.insert(key, area);
                }
            }
        }

        Self {
            version: stored.version,
            tasks,
            projects,
            areas,
            task_number_index,
        }
    }

    /// Convert from working format (typed HashMaps) to storage format (KV entries).
    pub fn to_stored(&self) -> StoredStore {
        let mut entries = HashMap::new();

        for (key, task) in &self.tasks {
            if let Ok(value) = serde_json::to_value(task) {
                entries.insert(key.clone(), value);
            }
        }
        for (key, project) in &self.projects {
            if let Ok(value) = serde_json::to_value(project) {
                entries.insert(key.clone(), value);
            }
        }
        for (key, area) in &self.areas {
            if let Ok(value) = serde_json::to_value(area) {
                entries.insert(key.clone(), value);
            }
        }

        StoredStore {
            version: self.version,
            entries,
        }
    }

    /// Compute the next available task number from current tasks.
    pub fn next_task_number(&self) -> u64 {
        self.tasks
            .values()
            .map(|t| t.task_number)
            .max()
            .unwrap_or(0)
            + 1
    }

    /// Add a task to the store, assigning it the next task_number.
    pub fn add_task(&mut self, mut task: Task) {
        task.task_number = self.next_task_number();
        let key = task.storage_key();
        self.task_number_index
            .insert(task.task_number, key.clone());
        self.tasks.insert(key, task);
    }

    /// Update an existing task in the store.
    pub fn update_task(&mut self, task: Task) {
        let key = task.storage_key();
        self.task_number_index
            .insert(task.task_number, key.clone());
        self.tasks.insert(key, task);
    }

    /// Add a project to the store. Returns the storage key.
    /// If a project with the same key already exists, returns the existing key.
    pub fn add_project(&mut self, project: Project) -> String {
        let key = project.storage_key();
        if !self.projects.contains_key(&key) {
            self.projects.insert(key.clone(), project);
        }
        key
    }

    /// Add an area to the store. Returns the storage key.
    /// If an area with the same key already exists, returns the existing key.
    pub fn add_area(&mut self, area: Area) -> String {
        let key = area.storage_key();
        if !self.areas.contains_key(&key) {
            self.areas.insert(key.clone(), area);
        }
        key
    }

    /// Get a task by its storage key.
    pub fn get_task(&self, key: &str) -> Option<&Task> {
        self.tasks.get(key)
    }

    /// Look up a task by its user-facing task_number (O(1) via secondary index).
    pub fn get_task_by_number(&self, number: u64) -> Option<&Task> {
        let key = self.task_number_index.get(&number)?;
        self.tasks.get(key)
    }

    /// Get the storage key for a task number.
    pub fn get_task_key_by_number(&self, number: u64) -> Option<&String> {
        self.task_number_index.get(&number)
    }

    /// Get a project by its storage key.
    pub fn get_project(&self, key: &str) -> Option<&Project> {
        self.projects.get(key)
    }

    /// Get an area by its storage key.
    pub fn get_area(&self, key: &str) -> Option<&Area> {
        self.areas.get(key)
    }

    /// Get all active (non-deleted) tasks.
    pub fn get_active_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values().filter(|t| t.deleted_at.is_none())
    }

    /// Get all active (non-deleted) projects.
    pub fn get_active_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values().filter(|p| p.deleted_at.is_none())
    }

    /// Get all active (non-deleted) areas.
    pub fn get_active_areas(&self) -> impl Iterator<Item = &Area> {
        self.areas.values().filter(|a| a.deleted_at.is_none())
    }

    /// Get all deleted tasks (for trash view).
    pub fn get_deleted_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values().filter(|t| t.deleted_at.is_some())
    }

    /// Get all deleted projects (for trash view).
    pub fn get_deleted_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values().filter(|p| p.deleted_at.is_some())
    }

    /// Get all deleted areas (for trash view).
    pub fn get_deleted_areas(&self) -> impl Iterator<Item = &Area> {
        self.areas.values().filter(|a| a.deleted_at.is_some())
    }

    /// Get a mutable task by its storage key.
    pub fn get_task_mut(&mut self, key: &str) -> Option<&mut Task> {
        self.tasks.get_mut(key)
    }

    /// Get a mutable project by its storage key.
    pub fn get_project_mut(&mut self, key: &str) -> Option<&mut Project> {
        self.projects.get_mut(key)
    }

    /// Get a mutable area by its storage key.
    pub fn get_area_mut(&mut self, key: &str) -> Option<&mut Area> {
        self.areas.get_mut(key)
    }

    /// Find tasks belonging to a project (by project storage key).
    pub fn get_tasks_for_project(&self, project_key: &str) -> impl Iterator<Item = &Task> {
        let key = project_key.to_string();
        self.tasks
            .values()
            .filter(move |t| t.project_key.as_deref() == Some(key.as_str()))
    }

    /// Find projects belonging to an area (by area storage key).
    pub fn get_projects_for_area(&self, area_key: &str) -> impl Iterator<Item = &Project> {
        let key = area_key.to_string();
        self.projects
            .values()
            .filter(move |p| p.area_key.as_deref() == Some(key.as_str()))
    }

    /// Find tasks directly belonging to an area (no project).
    pub fn get_tasks_for_area(&self, area_key: &str) -> impl Iterator<Item = &Task> {
        let key = area_key.to_string();
        self.tasks
            .values()
            .filter(move |t| t.area_key.as_deref() == Some(key.as_str()) && t.project_key.is_none())
    }

    /// Find non-deleted subtasks of a given parent task.
    pub fn get_subtasks(&self, parent_key: &str) -> impl Iterator<Item = &Task> {
        let key = parent_key.to_string();
        self.tasks
            .values()
            .filter(move |t| t.parent_task_key.as_deref() == Some(key.as_str()) && t.deleted_at.is_none())
    }

    /// Returns true if the task has at least one incomplete, non-deleted subtask.
    pub fn has_incomplete_subtasks(&self, task_key: &str) -> bool {
        self.get_subtasks(task_key)
            .any(|t| t.completed_at.is_none())
    }

    /// Returns true if the task has at least one incomplete, non-deleted dependency.
    pub fn is_task_blocked(&self, task: &Task) -> bool {
        task.depends_on.iter().any(|dep_key| {
            self.tasks.get(dep_key).is_some_and(|dep| {
                dep.completed_at.is_none() && dep.deleted_at.is_none()
            })
        })
    }

    /// Returns all tasks that are directly blocking the given task (incomplete, non-deleted).
    pub fn get_blockers(&self, task: &Task) -> Vec<&Task> {
        task.depends_on
            .iter()
            .filter_map(|dep_key| self.tasks.get(dep_key.as_str()))
            .filter(|dep| dep.completed_at.is_none() && dep.deleted_at.is_none())
            .collect()
    }

    /// Returns all tasks that depend on the given task (tasks that this task is blocking).
    pub fn get_blocking(&self, task_key: &str) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|t| t.depends_on.iter().any(|d| d == task_key))
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

    /// Rename a project: tombstone old key, create new key, update all task references.
    pub fn rename_project(&mut self, old_name: &str, new_name: &str) -> Option<String> {
        let old_key = format!("project/{}", old_name.to_lowercase());
        let new_key = format!("project/{}", new_name.to_lowercase());

        let mut project = self.projects.remove(&old_key)?;

        // Tombstone old entry
        let mut tombstone = project.clone();
        tombstone.deleted_at = Some(jiff::Timestamp::now());
        tombstone.renamed_to = Some(new_key.clone());
        self.projects.insert(old_key.clone(), tombstone);

        // Create new entry
        project.name = new_name.to_string();
        project.previous_key = Some(old_key.clone());
        project.modified_at = saku_storage::timestamp::HybridTimestamp::now(
            project.modified_at.lamport + 1,
            project.modified_at.device_id.clone(),
        );
        self.projects.insert(new_key.clone(), project);

        // Update task references
        for task in self.tasks.values_mut() {
            if task.project_key.as_deref() == Some(old_key.as_str()) {
                task.project_key = Some(new_key.clone());
            }
        }

        Some(new_key)
    }

    /// Rename an area: tombstone old key, create new key, update all project and task references.
    pub fn rename_area(&mut self, old_name: &str, new_name: &str) -> Option<String> {
        let old_key = format!("area/{}", old_name.to_lowercase());
        let new_key = format!("area/{}", new_name.to_lowercase());

        let mut area = self.areas.remove(&old_key)?;

        // Tombstone old entry
        let mut tombstone = area.clone();
        tombstone.deleted_at = Some(jiff::Timestamp::now());
        tombstone.renamed_to = Some(new_key.clone());
        self.areas.insert(old_key.clone(), tombstone);

        // Create new entry
        area.name = new_name.to_string();
        area.previous_key = Some(old_key.clone());
        area.modified_at = saku_storage::timestamp::HybridTimestamp::now(
            area.modified_at.lamport + 1,
            area.modified_at.device_id.clone(),
        );
        self.areas.insert(new_key.clone(), area);

        // Update project references
        for project in self.projects.values_mut() {
            if project.area_key.as_deref() == Some(old_key.as_str()) {
                project.area_key = Some(new_key.clone());
            }
        }

        // Update task references
        for task in self.tasks.values_mut() {
            if task.area_key.as_deref() == Some(old_key.as_str()) {
                task.area_key = Some(new_key.clone());
            }
        }

        Some(new_key)
    }
}
