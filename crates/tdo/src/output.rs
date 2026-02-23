use saku_tdo::models::{
    store::Store,
    task::{Task, When},
};
use serde::Serialize;

#[derive(Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Pretty,
    Json,
    Csv,
}

impl OutputFormat {
    pub fn from_flags(json: bool, csv: bool) -> Self {
        if json {
            Self::Json
        } else if csv {
            Self::Csv
        } else {
            Self::Pretty
        }
    }
}

/// Flattened task for machine-readable output (UUIDs resolved to names)
#[derive(Serialize)]
pub struct TaskOutput {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub when: String,
    pub scheduled_date: Option<String>,
    pub deadline: Option<String>,
    pub project: Option<String>,
    pub area: Option<String>,
    pub tags: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    /// Parent task number if this is a subtask, otherwise None
    pub parent_id: Option<u64>,
}

impl TaskOutput {
    pub fn from_task(task: &Task, store: &Store) -> Self {
        let status = if task.deleted_at.is_some() {
            "deleted"
        } else if task.completed_at.is_some() {
            "completed"
        } else {
            "active"
        };

        let (when_str, scheduled_date) = match &task.when {
            When::Inbox => ("inbox".to_string(), None),
            When::Someday => ("someday".to_string(), None),
            When::Scheduled { date } => ("scheduled".to_string(), Some(date.to_string())),
            When::LegacyToday { .. } => ("scheduled".to_string(), None),
            When::LegacyAnytime => ("someday".to_string(), None),
        };

        let project = task
            .project_id
            .and_then(|id| store.get_project(id))
            .map(|p| p.name.clone());

        let area = task
            .area_id
            .and_then(|id| store.get_area(id))
            .map(|a| a.name.clone())
            .or_else(|| {
                // Resolve area through project if task has no direct area
                task.project_id
                    .and_then(|pid| store.get_project(pid))
                    .and_then(|p| p.area_id)
                    .and_then(|aid| store.get_area(aid))
                    .map(|a| a.name.clone())
            });

        let parent_id = task
            .parent_task_id
            .and_then(|id| store.get_task(id))
            .map(|t| t.task_number);

        Self {
            id: task.task_number,
            title: task.title.clone(),
            status: status.to_string(),
            when: when_str,
            scheduled_date,
            deadline: task.deadline.map(|d| d.to_string()),
            project,
            area,
            tags: task.tags.join("|"),
            notes: task.notes.clone(),
            created_at: task.created_at.to_string(),
            completed_at: task.completed_at.map(|t| t.to_string()),
            parent_id,
        }
    }
}

/// For `list areas`
#[derive(Serialize)]
pub struct AreaOutput {
    pub name: String,
    pub project_count: usize,
    pub task_count: usize,
}

/// For `list projects`
#[derive(Serialize)]
pub struct ProjectOutput {
    pub name: String,
    pub area: Option<String>,
    pub task_count: usize,
}

/// For `list tags`
#[derive(Serialize)]
pub struct TagOutput {
    pub name: String,
    pub task_count: usize,
}

/// For `view trash` JSON (multi-type)
#[derive(Serialize)]
pub struct TrashOutput {
    pub tasks: Vec<TaskOutput>,
    pub projects: Vec<TrashProjectOutput>,
    pub areas: Vec<TrashAreaOutput>,
}

#[derive(Serialize)]
pub struct TrashProjectOutput {
    pub name: String,
}

#[derive(Serialize)]
pub struct TrashAreaOutput {
    pub name: String,
}

pub fn print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

pub fn print_csv<T: Serialize>(items: &[T]) {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for item in items {
        wtr.serialize(item).unwrap();
    }
    wtr.flush().unwrap();
}
