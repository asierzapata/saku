use saku_tdo::models::store::Store;
use saku_tdo::models::task::Task;

/// Recognized assignment values that mark a task for agent execution.
const AGENT_ASSIGNEES: &[&str] = &["agent", "wrk"];

/// Returns tasks from the store that are eligible for agent execution.
///
/// A task is executable when:
/// 1. It is assigned to an agent (`assigned_to` is "agent" or "wrk")
/// 2. It is not completed and not deleted
/// 3. It is not blocked by incomplete dependencies
///
/// Results are sorted by: deadline (soonest first), then task number.
pub fn pick_executable_tasks(store: &Store) -> Vec<&Task> {
    let mut tasks: Vec<&Task> = store
        .get_active_tasks()
        .filter(|t| t.completed_at.is_none())
        .filter(|t| {
            t.assigned_to
                .as_deref()
                .is_some_and(|a| AGENT_ASSIGNEES.contains(&a.to_lowercase().as_str()))
        })
        .filter(|t| !store.is_task_blocked(t))
        .collect();

    // Sort: deadline soonest first (None last), then by task number
    tasks.sort_by(|a, b| {
        let deadline_ord = match (a.deadline, b.deadline) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(da), Some(db)) => da.cmp(&db),
        };
        if deadline_ord != std::cmp::Ordering::Equal {
            return deadline_ord;
        }
        a.task_number.cmp(&b.task_number)
    });

    tasks
}
