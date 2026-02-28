use saku_storage::entity::Entity;
use saku_tdo::models::store::Store;
use saku_tdo::models::task::Task;
use saku_tdo::services::tasks::{CompleteTaskParameters, complete_task};
use saku_tdo::storage::Storage;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReporterError {
    #[error("Task #{0} not found")]
    TaskNotFound(u64),

    #[error("Task #{0} is already completed")]
    AlreadyCompleted(u64),

    #[error("Storage error: {0}")]
    Storage(#[from] saku_tdo::storage::StorageError),

    #[error("Failed to complete task: {0}")]
    CompleteFailed(String),
}

/// Claim a task for execution — sets `assigned_to = "wrk"` and moves to Today.
pub fn claim_task(
    store: &mut Store,
    storage: &impl Storage,
    task_number: u64,
) -> Result<Task, ReporterError> {
    let task = store
        .get_task_by_number(task_number)
        .ok_or(ReporterError::TaskNotFound(task_number))?;

    if task.completed_at.is_some() {
        return Err(ReporterError::AlreadyCompleted(task_number));
    }

    let task_key = task.storage_key();
    let mut updated = task.clone();
    updated.assigned_to = Some("wrk".to_string());

    // Move to Today if in Inbox or Someday
    use saku_tdo::models::task::When;
    match updated.when {
        When::Inbox | When::Someday => {
            let today = jiff::Zoned::now().date();
            updated.when = When::Scheduled { date: today };
        }
        _ => {}
    }

    updated.modified_at = saku_tdo::sync_clock::next_modified_at();

    store.tasks.insert(task_key, updated.clone());
    storage.save(store)?;

    Ok(updated)
}

/// Report successful execution — complete the task with a summary note.
pub fn report_success(
    store: &mut Store,
    storage: &impl Storage,
    task_number: u64,
    summary: &str,
) -> Result<Task, ReporterError> {
    let note = format!("[wrk] Completed successfully.\n{}", summary);

    let params = CompleteTaskParameters {
        task_number_or_fuzzy_name: task_number.to_string(),
        stop: false,
        note: Some(note),
    };

    match complete_task(store, storage, params) {
        Ok(result) => Ok(result.task),
        Err(e) => Err(ReporterError::CompleteFailed(e.to_string())),
    }
}

/// Report successful execution but mark for review (don't complete).
pub fn report_needs_review(
    store: &mut Store,
    storage: &impl Storage,
    task_number: u64,
    summary: &str,
) -> Result<Task, ReporterError> {
    let task = store
        .get_task_by_number(task_number)
        .ok_or(ReporterError::TaskNotFound(task_number))?;

    let task_key = task.storage_key();
    let mut updated = task.clone();

    let note = format!("[wrk] Execution completed — needs review.\n{}", summary);
    updated.notes = Some(match &updated.notes {
        Some(existing) => format!("{}\n\n{}", existing, note),
        None => note,
    });

    // Add needs-review tag if not already present
    if !updated.tags.iter().any(|t| t == "needs-review") {
        updated.tags.push("needs-review".to_string());
    }

    updated.modified_at = saku_tdo::sync_clock::next_modified_at();

    store.tasks.insert(task_key, updated.clone());
    storage.save(store)?;

    Ok(updated)
}

/// Report failure — append error details to the task notes, leave in current state.
pub fn report_failure(
    store: &mut Store,
    storage: &impl Storage,
    task_number: u64,
    error: &str,
) -> Result<Task, ReporterError> {
    let task = store
        .get_task_by_number(task_number)
        .ok_or(ReporterError::TaskNotFound(task_number))?;

    let task_key = task.storage_key();
    let mut updated = task.clone();

    let note = format!("[wrk] Execution failed.\n{}", error);
    updated.notes = Some(match &updated.notes {
        Some(existing) => format!("{}\n\n{}", existing, note),
        None => note,
    });

    updated.modified_at = saku_tdo::sync_clock::next_modified_at();

    store.tasks.insert(task_key, updated.clone());
    storage.save(store)?;

    Ok(updated)
}
