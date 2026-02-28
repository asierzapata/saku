use jiff::civil::Date;
use saku_storage::entity::Entity;
use saku_tdo::models::{
    store::Store,
    task::{Task, When, is_pending_on},
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

/// Flattened task for machine-readable output (keys resolved to names)
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
    pub defer_until: Option<String>,
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
            .project_key
            .as_deref()
            .and_then(|key| store.get_project(key))
            .map(|p| p.name.clone());

        let area = task
            .area_key
            .as_deref()
            .and_then(|key| store.get_area(key))
            .map(|a| a.name.clone())
            .or_else(|| {
                // Resolve area through project if task has no direct area
                task.project_key
                    .as_deref()
                    .and_then(|key| store.get_project(key))
                    .and_then(|p| p.area_key.as_deref())
                    .and_then(|aid| store.get_area(aid))
                    .map(|a| a.name.clone())
            });

        let parent_id = task
            .parent_task_key
            .as_deref()
            .and_then(|key| store.get_task(key))
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
            defer_until: task.defer_until.map(|d| d.to_string()),
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

/// Summary of a single project for `tdo context`
#[derive(Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub area: Option<String>,
    pub task_count: u64,
}

/// Summary counts for today's tasks
#[derive(Serialize)]
pub struct TodaySummary {
    pub total: u64,
    pub ready: u64,
    pub blocked: u64,
}

/// Reference to a blocking task
#[derive(Serialize)]
pub struct BlockerRef {
    pub task_number: u64,
    pub title: String,
}

/// A blocked task with its blocker references
#[derive(Serialize)]
pub struct BlockedTaskSummary {
    pub task_number: u64,
    pub title: String,
    pub waiting_on: Vec<BlockerRef>,
}

/// Full context snapshot for `tdo context`
#[derive(Serialize)]
pub struct ContextOutput {
    pub date: String,
    pub active_projects: Vec<ProjectSummary>,
    pub today: TodaySummary,
    pub ready_tasks: Vec<TaskOutput>,
    pub blocked_tasks: Vec<BlockedTaskSummary>,
    pub inbox_count: u64,
    pub needs_review_count: u64,
    pub recent_completions: Vec<TaskOutput>,
    pub overdue_tasks: Vec<TaskOutput>,
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

/// Build a complete context snapshot from the store.
pub fn build_context(store: &Store, today: Date) -> ContextOutput {
    // --- Active projects with task counts ---
    let mut active_projects: Vec<ProjectSummary> = store
        .get_active_projects()
        .map(|p| {
            let project_key = p.storage_key();
            let task_count = store
                .get_tasks_for_project(&project_key)
                .filter(|t| t.deleted_at.is_none() && t.completed_at.is_none())
                .count() as u64;
            let area = p
                .area_key
                .as_deref()
                .and_then(|aid| store.get_area(aid))
                .map(|a| a.name.clone());
            ProjectSummary {
                name: p.name.clone(),
                area,
                task_count,
            }
        })
        .filter(|p| p.task_count > 0)
        .collect();
    active_projects.sort_by(|a, b| a.name.cmp(&b.name));

    // --- Collect today's tasks (scheduled today or recurring today), excluding subtasks ---
    let today_tasks: Vec<&Task> = store
        .get_active_tasks()
        .filter(|t| t.completed_at.is_none() && t.parent_task_key.is_none())
        .filter(|t| t.defer_until.is_none() || t.defer_until.unwrap() <= today)
        .filter(|t| {
            let scheduled_today = match t.when {
                When::Scheduled { date, .. } => date == today,
                _ => false,
            };
            let deadline_today = t.deadline == Some(today);
            let recurring_today = is_pending_on(t, today);
            scheduled_today || deadline_today || recurring_today
        })
        .collect();

    let today_ready: Vec<&Task> = today_tasks
        .iter()
        .filter(|t| !store.is_task_blocked(t))
        .copied()
        .collect();
    let today_blocked: Vec<&Task> = today_tasks
        .iter()
        .filter(|t| store.is_task_blocked(t))
        .copied()
        .collect();

    let today_summary = TodaySummary {
        total: today_tasks.len() as u64,
        ready: today_ready.len() as u64,
        blocked: today_blocked.len() as u64,
    };

    // --- Ready tasks (today or overdue, unblocked, max 10) ---
    let overdue_tasks: Vec<&Task> = store
        .get_active_tasks()
        .filter(|t| {
            t.completed_at.is_none()
                && t.parent_task_key.is_none()
                && t.recurrence.is_none()
                && (t.defer_until.is_none() || t.defer_until.unwrap() <= today)
        })
        .filter(|t| {
            let scheduled_overdue = match t.when {
                When::Scheduled { date, .. } => date < today,
                _ => false,
            };
            let deadline_overdue = t.deadline.is_some_and(|d| d < today);
            scheduled_overdue || deadline_overdue
        })
        .collect();

    let mut ready_tasks: Vec<&Task> = today_ready
        .iter()
        .chain(
            overdue_tasks
                .iter()
                .filter(|t| !store.is_task_blocked(t)),
        )
        .copied()
        .collect();
    // Deduplicate by task number
    ready_tasks.sort_by_key(|t| t.task_number);
    ready_tasks.dedup_by_key(|t| t.storage_key_suffix.clone());
    ready_tasks.truncate(10);

    let ready_task_outputs: Vec<TaskOutput> = ready_tasks
        .iter()
        .map(|t| TaskOutput::from_task(t, store))
        .collect();

    // --- Blocked tasks with blocker references ---
    let blocked_task_summaries: Vec<BlockedTaskSummary> = today_blocked
        .iter()
        .map(|t| {
            let blockers = store.get_blockers(t);
            BlockedTaskSummary {
                task_number: t.task_number,
                title: t.title.clone(),
                waiting_on: blockers
                    .iter()
                    .map(|b| BlockerRef {
                        task_number: b.task_number,
                        title: b.title.clone(),
                    })
                    .collect(),
            }
        })
        .collect();

    // --- Inbox counts ---
    let inbox_tasks: Vec<&Task> = store
        .get_active_tasks()
        .filter(|t| {
            t.completed_at.is_none()
                && matches!(t.when, When::Inbox)
                && (t.defer_until.is_none() || t.defer_until.unwrap() <= today)
        })
        .collect();
    let inbox_count = inbox_tasks.len() as u64;
    let needs_review_count = inbox_tasks
        .iter()
        .filter(|t| {
            t.tags
                .iter()
                .any(|tag| tag.to_lowercase() == "needs-review")
        })
        .count() as u64;

    // --- Recent completions (last 48h, max 5) ---
    let threshold_48h = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(48))
        .unwrap_or(jiff::Timestamp::now());
    let mut recent_completions: Vec<&Task> = store
        .tasks
        .values()
        .filter(|t| {
            t.deleted_at.is_none()
                && t.parent_task_key.is_none()
                && t.completed_at.is_some_and(|c| c >= threshold_48h)
        })
        .collect();
    recent_completions.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
    recent_completions.truncate(5);
    let recent_completion_outputs: Vec<TaskOutput> = recent_completions
        .iter()
        .map(|t| TaskOutput::from_task(t, store))
        .collect();

    // --- Overdue tasks ---
    let overdue_task_outputs: Vec<TaskOutput> = overdue_tasks
        .iter()
        .map(|t| TaskOutput::from_task(t, store))
        .collect();

    ContextOutput {
        date: today.to_string(),
        active_projects,
        today: today_summary,
        ready_tasks: ready_task_outputs,
        blocked_tasks: blocked_task_summaries,
        inbox_count,
        needs_review_count,
        recent_completions: recent_completion_outputs,
        overdue_tasks: overdue_task_outputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use saku_tdo::models::project::Project;

    fn make_store() -> Store {
        Store::default()
    }

    fn make_task(title: &str, when: When) -> Task {
        Task {
            storage_key_suffix: uuid::Uuid::new_v4()
                .to_string()
                .replace('-', "")
                .chars()
                .take(8)
                .collect(),
            title: title.to_string(),
            when,
            ..Task::default()
        }
    }

    #[test]
    fn empty_store_returns_zeroes() {
        let store = make_store();
        let ctx = build_context(&store, date(2026, 2, 27));

        assert_eq!(ctx.date, "2026-02-27");
        assert!(ctx.active_projects.is_empty());
        assert_eq!(ctx.today.total, 0);
        assert_eq!(ctx.today.ready, 0);
        assert_eq!(ctx.today.blocked, 0);
        assert!(ctx.ready_tasks.is_empty());
        assert!(ctx.blocked_tasks.is_empty());
        assert_eq!(ctx.inbox_count, 0);
        assert_eq!(ctx.needs_review_count, 0);
        assert!(ctx.recent_completions.is_empty());
        assert!(ctx.overdue_tasks.is_empty());
    }

    #[test]
    fn mixed_state_produces_correct_counts() {
        let mut store = make_store();
        let today = date(2026, 2, 27);

        // A project
        let proj = Project {
            name: "auth-service".to_string(),
            ..Project::default()
        };
        let proj_key = store.add_project(proj.clone());

        // Today task (ready)
        let mut t1 = make_task("Refactor auth", When::Scheduled { date: today });
        t1.project_key = Some(proj_key.clone());
        store.add_task(t1);

        // Today task (blocked) — depends on t1
        let t1_key = store
            .tasks
            .iter()
            .find(|(_, t)| t.title == "Refactor auth")
            .map(|(k, _)| k.clone())
            .unwrap();
        let mut t2 = make_task("Deploy auth", When::Scheduled { date: today });
        t2.project_key = Some(proj_key.clone());
        t2.depends_on = vec![t1_key];
        store.add_task(t2);

        // Inbox task
        store.add_task(make_task("Review PR", When::Inbox));

        // Inbox task with needs-review tag
        let mut t_review = make_task("Triage bug", When::Inbox);
        t_review.tags = vec!["needs-review".to_string()];
        store.add_task(t_review);

        // Overdue task (scheduled yesterday)
        store.add_task(make_task(
            "Fix memory leak",
            When::Scheduled {
                date: date(2026, 2, 25),
            },
        ));

        let ctx = build_context(&store, today);

        // Today summary: 2 tasks scheduled today (1 ready, 1 blocked)
        assert_eq!(ctx.today.total, 2);
        assert_eq!(ctx.today.ready, 1);
        assert_eq!(ctx.today.blocked, 1);

        // Inbox: 2 items, 1 needs-review
        assert_eq!(ctx.inbox_count, 2);
        assert_eq!(ctx.needs_review_count, 1);

        // Overdue: 1 task
        assert_eq!(ctx.overdue_tasks.len(), 1);
        assert_eq!(ctx.overdue_tasks[0].title, "Fix memory leak");

        // Ready tasks: 1 from today + 1 overdue = 2
        assert_eq!(ctx.ready_tasks.len(), 2);

        // Blocked tasks: 1 with 1 blocker
        assert_eq!(ctx.blocked_tasks.len(), 1);
        assert_eq!(ctx.blocked_tasks[0].title, "Deploy auth");
        assert_eq!(ctx.blocked_tasks[0].waiting_on.len(), 1);
        assert_eq!(ctx.blocked_tasks[0].waiting_on[0].title, "Refactor auth");

        // Active projects: 1 with 2 active tasks
        assert_eq!(ctx.active_projects.len(), 1);
        assert_eq!(ctx.active_projects[0].name, "auth-service");
        assert_eq!(ctx.active_projects[0].task_count, 2);
    }

    #[test]
    fn blocked_tasks_reference_correct_blockers() {
        let mut store = make_store();
        let today = date(2026, 3, 1);

        let blocker1 = make_task("Write tests", When::Scheduled { date: today });
        store.add_task(blocker1.clone());
        let blocker1_key = store
            .tasks
            .iter()
            .find(|(_, t)| t.title == "Write tests")
            .map(|(k, _)| k.clone())
            .unwrap();

        let blocker2 = make_task("Code review", When::Scheduled { date: today });
        store.add_task(blocker2.clone());
        let blocker2_key = store
            .tasks
            .iter()
            .find(|(_, t)| t.title == "Code review")
            .map(|(k, _)| k.clone())
            .unwrap();

        let mut blocked = make_task("Deploy", When::Scheduled { date: today });
        blocked.depends_on = vec![blocker1_key, blocker2_key];
        store.add_task(blocked);

        let ctx = build_context(&store, today);

        assert_eq!(ctx.blocked_tasks.len(), 1);
        assert_eq!(ctx.blocked_tasks[0].waiting_on.len(), 2);

        let blocker_titles: Vec<&str> = ctx.blocked_tasks[0]
            .waiting_on
            .iter()
            .map(|b| b.title.as_str())
            .collect();
        assert!(blocker_titles.contains(&"Write tests"));
        assert!(blocker_titles.contains(&"Code review"));
    }

    #[test]
    fn recent_completions_within_48h() {
        let mut store = make_store();
        let today = date(2026, 2, 27);

        // Completed 1 hour ago — should appear
        let mut t_recent = make_task("Recent done", When::Scheduled { date: today });
        t_recent.completed_at = Some(
            jiff::Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_hours(1))
                .unwrap(),
        );
        store.add_task(t_recent);

        // Completed 3 days ago — should NOT appear
        let mut t_old = make_task("Old done", When::Scheduled { date: date(2026, 2, 24) });
        t_old.completed_at = Some(
            jiff::Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_hours(72))
                .unwrap(),
        );
        store.add_task(t_old);

        let ctx = build_context(&store, today);

        assert_eq!(ctx.recent_completions.len(), 1);
        assert_eq!(ctx.recent_completions[0].title, "Recent done");
    }

    #[test]
    fn max_limits_enforced() {
        let mut store = make_store();
        let today = date(2026, 2, 27);

        // Add 15 ready tasks for today — should cap at 10
        for i in 0..15 {
            store.add_task(make_task(
                &format!("Task {}", i),
                When::Scheduled { date: today },
            ));
        }

        // Add 8 completed tasks — should cap at 5
        for i in 0..8 {
            let mut t = make_task(
                &format!("Done {}", i),
                When::Scheduled { date: today },
            );
            t.completed_at = Some(
                jiff::Timestamp::now()
                    .checked_sub(jiff::SignedDuration::from_hours(1))
                    .unwrap(),
            );
            store.add_task(t);
        }

        let ctx = build_context(&store, today);

        assert!(ctx.ready_tasks.len() <= 10);
        assert!(ctx.recent_completions.len() <= 5);
    }
}
