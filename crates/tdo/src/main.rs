use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use colored::*;

use saku_storage::entity::Entity;

use saku_tdo::{
    models::task::{When, WhenInstantiationError, is_pending_on, next_pending_occurrence},
    recurrence_parser::parse_recurrence,
    services::{
        areas::{
            CreateAreaError, CreateAreaParameters, DeleteAreaError, DeleteAreaParameters,
            EditAreaError, EditAreaParameters, create_area, delete_area, edit_area,
        },
        projects::{
            CreateProjectError, CreateProjectParameters, DeleteProjectError,
            DeleteProjectParameters, EditProjectError, EditProjectParameters, create_project,
            delete_project, edit_project,
        },
        tasks::{
            AddTaskError, AddTaskParameters, CompleteTaskError, CompleteTaskParameters,
            DeleteTaskError, DeleteTaskParameters, DependTaskError, DependTaskParameters,
            EditTaskError, EditTaskParameters, MoveTaskError, MoveTaskParameters,
            RestoreTaskError, RestoreTaskParameters, add_task, complete_task, delete_task,
            depend_task, edit_task, move_task, restore_task,
        },
    },
    storage::{Storage, json::JsonFileStorage},
};

#[cfg(feature = "logging")]
use saku_tdo::logging;

mod output;

#[derive(Parser)]
#[command(
    name = "tdo",
    about = "A minimal and clean task manager for your terminal"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show today's tasks (including overdue)
    Today,

    /// List tasks in the inbox
    Inbox,

    /// Show upcoming tasks (future-dated)
    Upcoming,

    /// Show someday tasks
    Someday,

    /// Show completed tasks (last 14 days)
    Logbook,

    /// Show deleted items
    Trash,

    /// Show all active tasks
    All,

    /// View tasks and entities
    View {
        /// Output as JSON
        #[arg(long, short = 'j', conflicts_with = "csv")]
        json: bool,

        /// Output as CSV
        #[arg(long, short = 'c', conflicts_with = "json")]
        csv: bool,

        /// Watch for changes and re-render automatically (pretty output only)
        #[arg(long, short = 'w', conflicts_with_all = ["json", "csv"])]
        watch: bool,

        /// Include completed tasks (not applicable to Logbook or Trash views)
        #[arg(long)]
        all: bool,

        /// Filter by project name
        #[arg(long, short = 'p', value_name = "NAME")]
        project: Option<String>,

        /// Filter by tag (can be used multiple times, OR logic within tags)
        #[arg(long, short = 't', value_name = "NAME", action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Filter by area name
        #[arg(long, short = 'a', value_name = "NAME")]
        area: Option<String>,

        /// Show only tasks that are not blocked
        #[arg(long, short = 'r')]
        ready: bool,

        /// What to view (today, inbox, all, someday, upcoming, deadlines, logbook, trash, recurring, deferred, area, project, tag, task)
        entity: String,

        /// Name or ID (required for: area, project, tag, task)
        name: Option<String>,
    },

    /// Add a new task
    Add {
        /// Task title
        title: String,

        /// Schedule for today
        #[arg(long)]
        today: bool,

        /// Schedule for tomorrow
        #[arg(long)]
        tomorrow: bool,

        /// Schedule for next week (Monday of next week)
        #[arg(long)]
        next_week: bool,

        /// Defer to someday
        #[arg(long)]
        someday: bool,

        /// Schedule for a specific date (e.g., "monday", "next friday", "2026-03-15")
        #[arg(long)]
        on: Option<String>,

        /// Set a hard deadline (e.g., "friday", "2026-03-20")
        #[arg(long)]
        due: Option<String>,

        /// Assign to a project
        #[arg(short, long)]
        project: Option<String>,

        /// Assign to an area
        #[arg(short, long)]
        area: Option<String>,

        /// Add tags (can be used multiple times)
        #[arg(short, long, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Add notes
        #[arg(short, long)]
        notes: Option<String>,

        /// Defer until a specific date (task hidden from today/inbox until then)
        #[arg(long, value_name = "DATE")]
        defer_until: Option<String>,

        /// Recurrence pattern, e.g. "daily", "monday", "mon,wed,fri", "1st of month"
        #[arg(long, value_name = "PATTERN")]
        every: Option<String>,

        /// End date for recurrence (e.g. "2026-12-31")
        #[arg(long, value_name = "DATE")]
        until: Option<String>,

        /// Make this a subtask of another task (by task number)
        #[arg(long)]
        parent: Option<u64>,
    },

    /// Moves one or more tasks
    Move {
        /// Task number(s) - provide multiple to move several tasks at once
        #[arg(num_args(1..))]
        task_numbers: Vec<String>,

        /// Schedule for today
        #[arg(long)]
        today: bool,

        /// Schedule for tomorrow
        #[arg(long)]
        tomorrow: bool,

        /// Schedule for next week (Monday of next week)
        #[arg(long)]
        next_week: bool,

        /// Defer to someday
        #[arg(long)]
        someday: bool,

        /// Schedule for a specific date (e.g., "monday", "next friday", "2026-03-15")
        #[arg(long)]
        on: Option<String>,

        /// Set a hard deadline (e.g., "friday", "2026-03-20")
        #[arg(long)]
        due: Option<String>,

        /// Remove scheduled date
        #[arg(long)]
        clear_schedule: bool,

        /// Remove deadline
        #[arg(long)]
        clear_deadline: bool,

        /// Assign to a project
        #[arg(short, long)]
        project: Option<String>,

        /// Assign to an area
        #[arg(short, long)]
        area: Option<String>,

        /// Remove project assignment
        #[arg(long)]
        clear_project: bool,

        /// Remove area assignment
        #[arg(long)]
        clear_area: bool,

        /// Add tags (can be used multiple times)
        #[arg(short, long, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Add notes
        #[arg(short, long)]
        notes: Option<String>,

        /// Defer until a specific date (task hidden from today/inbox until then)
        #[arg(long, value_name = "DATE")]
        defer_until: Option<String>,

        /// Remove defer-until date
        #[arg(long)]
        clear_defer: bool,

        /// Recurrence pattern, e.g. "daily", "monday", "mon,wed,fri", "1st of month"
        #[arg(long, value_name = "PATTERN")]
        every: Option<String>,

        /// End date for recurrence (e.g. "2026-12-31")
        #[arg(long, value_name = "DATE")]
        until: Option<String>,

        /// Remove recurrence from this task
        #[arg(long)]
        clear_recurrence: bool,
    },

    /// Complete one or more tasks
    Done {
        /// Task number(s) or fuzzy name(s) - provide multiple to complete several tasks at once
        #[arg(num_args(1..))]
        task_numbers_or_fuzzy_names: Vec<String>,

        /// Permanently cancel a recurring task (stops it from repeating)
        #[arg(long)]
        stop: bool,

        /// Append a note to the task when completing it
        #[arg(long, short = 'n')]
        note: Option<String>,
    },

    /// Add or remove a dependency between tasks
    Depend {
        /// Task number of the blocked task
        task_number: u64,

        /// Task number this task depends on (adds a dependency)
        #[arg(long)]
        on: Option<u64>,

        /// Task number to remove as a dependency
        #[arg(long)]
        remove: Option<u64>,
    },

    /// Delete one or more tasks (move to trash)
    Delete {
        /// Task number(s) or fuzzy name(s) - provide multiple to delete several tasks at once
        #[arg(num_args(1..))]
        task_numbers_or_fuzzy_names: Vec<String>,
    },

    /// Restore one or more tasks from trash
    Restore {
        /// Task number(s) - provide multiple to restore several tasks at once
        #[arg(num_args(1..))]
        task_numbers: Vec<String>,
    },

    /// Create a new area or project
    Create {
        #[command(subcommand)]
        entity: CreateEntity,
    },

    /// Show details of an area, project, or tag
    Show {
        /// Output as JSON
        #[arg(long, conflicts_with = "csv")]
        json: bool,

        /// Output as CSV
        #[arg(long, conflicts_with = "json")]
        csv: bool,

        #[command(subcommand)]
        entity: ShowEntity,
    },

    /// Remove an area or project
    Remove {
        #[command(subcommand)]
        entity: RemoveEntity,
    },

    /// Edit an area, project, or task
    Edit {
        #[command(subcommand)]
        entity: EditEntity,
    },

    /// List all areas, projects, or tags
    List {
        /// Output as JSON
        #[arg(long, conflicts_with = "csv")]
        json: bool,

        /// Output as CSV
        #[arg(long, conflicts_with = "json")]
        csv: bool,

        #[command(subcommand)]
        entity: ListEntity,
    },

    /// Search tasks by title (and optionally notes)
    Search {
        /// Search query (substring match)
        query: String,

        /// Also search inside task notes
        #[arg(long)]
        notes: bool,

        /// Output as JSON
        #[arg(long, conflicts_with = "csv")]
        json: bool,

        /// Output as CSV
        #[arg(long, conflicts_with = "json")]
        csv: bool,
    },

    /// Show a full situational snapshot (today, blockers, inbox, overdue, projects)
    Context {
        /// Output as JSON
        #[arg(long, conflicts_with = "csv")]
        json: bool,

        /// Output as CSV (not supported, reserved for consistency)
        #[arg(long, conflicts_with = "json")]
        csv: bool,
    },

    /// Generate shell completion script
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Manage sync settings
    #[cfg(feature = "sync")]
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
}

#[cfg(feature = "sync")]
#[derive(Debug, Subcommand)]
enum SyncAction {
    /// Log in to a sync server
    Login {
        /// Server URL (e.g. http://localhost:8080)
        #[arg(long)]
        server: String,

        /// Email address
        #[arg(long)]
        email: String,
    },

    /// Log out and clear sync credentials
    Logout,

    /// Show sync status
    Status,
}

#[derive(Debug, Subcommand)]
enum CreateEntity {
    /// Create a new area
    Area {
        /// Name of the area
        name: String,
    },
    /// Create a new project
    Project {
        /// Name of the project
        name: String,
        /// Assign to an area
        #[arg(short, long)]
        area: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ShowEntity {
    /// Show projects in an area
    Area {
        /// Name of the area
        name: String,
    },
    /// Show tasks in a project
    Project {
        /// Name of the project
        name: String,
    },
    /// Show tasks with a specific tag
    Tag {
        /// Name of the tag
        name: String,
    },
}

#[derive(Debug)]
enum ViewEntity {
    Today,
    Inbox,
    Upcoming,
    Someday,
    Deadlines,
    Logbook,
    Trash,
    All,
    Recurring,
    Deferred,
    Area { name: String },
    Project { name: String },
    Tag { name: String },
    Task { id: String },
}

fn parse_view_entity(entity: &str, name: Option<String>) -> Result<ViewEntity, String> {
    match entity.to_lowercase().as_str() {
        "today" => Ok(ViewEntity::Today),
        "inbox" => Ok(ViewEntity::Inbox),
        "upcoming" => Ok(ViewEntity::Upcoming),
        "someday" => Ok(ViewEntity::Someday),
        "deadlines" => Ok(ViewEntity::Deadlines),
        "logbook" => Ok(ViewEntity::Logbook),
        "trash" => Ok(ViewEntity::Trash),
        "all" => Ok(ViewEntity::All),
        "recurring" => Ok(ViewEntity::Recurring),
        "deferred" => Ok(ViewEntity::Deferred),
        "area" => name
            .ok_or_else(|| "area requires a name argument".to_string())
            .map(|n| ViewEntity::Area { name: n }),
        "project" => name
            .ok_or_else(|| "project requires a name argument".to_string())
            .map(|n| ViewEntity::Project { name: n }),
        "tag" => name
            .ok_or_else(|| "tag requires a name argument".to_string())
            .map(|n| ViewEntity::Tag { name: n }),
        "task" => name
            .ok_or_else(|| "task requires a number or name argument".to_string())
            .map(|n| ViewEntity::Task { id: n }),
        other => Err(format!(
            "unknown view: '{}'. Options: today, inbox, all, someday, upcoming, deadlines, logbook, trash, recurring, deferred, area, project, tag, task",
            other
        )),
    }
}

#[derive(Debug, Subcommand)]
enum RemoveEntity {
    /// Remove an area (and all its projects/tasks)
    Area {
        /// Name of the area
        name: String,
    },
    /// Remove a project (and all its tasks)
    Project {
        /// Name of the project
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum EditEntity {
    /// Edit an area
    Area {
        /// Current name of the area
        name: String,
        /// New name for the area
        #[arg(long)]
        new_name: String,
    },
    /// Edit a project
    Project {
        /// Current name of the project
        name: String,
        /// New name for the project
        #[arg(long)]
        new_name: Option<String>,
        /// Assign to a different area (or empty string to remove area)
        #[arg(long)]
        area: Option<String>,
    },
    /// Edit a task in your editor
    Task {
        /// Task number or fuzzy name
        task_number_or_fuzzy_name: String,
    },
}

#[derive(Debug, Subcommand)]
enum ListEntity {
    /// List all areas
    Areas,
    /// List all projects
    Projects,
    /// List all tags
    Tags,
}

/// Get the hostname of this machine for device naming.
#[cfg(feature = "sync")]
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Filters to apply to task views.
struct ViewFilters {
    project: Option<String>,
    tags: Vec<String>,
    area: Option<String>,
    ready: bool,
}

impl ViewFilters {
    fn is_empty(&self) -> bool {
        self.project.is_none() && self.tags.is_empty() && self.area.is_none() && !self.ready
    }
}

/// Apply view filters to a list of tasks.
fn filter_tasks<'a>(
    tasks: Vec<&'a saku_tdo::models::task::Task>,
    filters: &ViewFilters,
    store: &saku_tdo::models::store::Store,
) -> Vec<&'a saku_tdo::models::task::Task> {
    if filters.is_empty() {
        return tasks;
    }

    // Resolve project name to storage key
    let project_key = filters.project.as_ref().and_then(|name| {
        store
            .get_active_projects()
            .find(|p| p.name.to_lowercase().contains(&name.to_lowercase()))
            .map(|p| p.storage_key())
    });

    // Resolve area name to storage key
    let area_key = filters.area.as_ref().and_then(|name| {
        store
            .get_active_areas()
            .find(|a| a.name.to_lowercase().contains(&name.to_lowercase()))
            .map(|a| a.storage_key())
    });

    // Collect project keys belonging to the area (for transitive area filtering)
    let area_project_keys: Vec<String> = if let Some(ref akey) = area_key {
        store
            .get_projects_for_area(akey)
            .filter(|p| p.deleted_at.is_none())
            .map(|p| p.storage_key())
            .collect()
    } else {
        vec![]
    };

    tasks
        .into_iter()
        .filter(|t| {
            // --project filter
            if filters.project.is_some() {
                if let Some(ref pkey) = project_key {
                    if t.project_key.as_deref() != Some(pkey.as_str()) {
                        return false;
                    }
                } else {
                    return false; // project name didn't resolve
                }
            }

            // --tag filter (OR within tags)
            if !filters.tags.is_empty()
                && !filters.tags.iter().any(|filter_tag| {
                    t.tags
                        .iter()
                        .any(|task_tag| task_tag.to_lowercase() == filter_tag.to_lowercase())
                })
            {
                return false;
            }

            // --area filter (direct area or project belongs to area)
            if filters.area.is_some() {
                if let Some(ref akey) = area_key {
                    let direct_match = t.area_key.as_deref() == Some(akey.as_str());
                    let via_project = t
                        .project_key
                        .as_ref()
                        .is_some_and(|pkey| area_project_keys.contains(pkey));
                    if !direct_match && !via_project {
                        return false;
                    }
                } else {
                    return false; // area name didn't resolve
                }
            }

            // --ready filter
            if filters.ready && store.is_task_blocked(t) {
                return false;
            }

            true
        })
        .collect()
}

/// Render any ViewEntity in pretty (terminal) format.
/// New ViewEntity arms get watch support for free.
fn render_view_pretty(entity: &ViewEntity, store: &saku_tdo::models::store::Store, include_completed: bool, filters: &ViewFilters) {
    match entity {
        ViewEntity::Today => render_today_view(store, include_completed, filters),
        ViewEntity::Inbox => {
            let today = jiff::Zoned::now().date();
            let inbox_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Inbox))
                .filter(|t| include_completed || t.completed_at.is_none())
                .filter(|t| t.defer_until.is_none() || t.defer_until.unwrap() <= today)
                .collect();
            let inbox_tasks = filter_tasks(inbox_tasks, filters, store);
            let inbox_tasks =
                saku_tdo::models::task::order_tasks_with_store(inbox_tasks, store);
            if inbox_tasks.is_empty() {
                println!("Inbox is empty");
            } else {
                saku_tdo::ui::render_view_header("Inbox", inbox_tasks.len());
                for task in inbox_tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
            }
        }
        ViewEntity::Someday => {
            let someday_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Someday))
                .filter(|t| include_completed || t.completed_at.is_none())
                .collect();
            let someday_tasks = filter_tasks(someday_tasks, filters, store);
            let someday_tasks =
                saku_tdo::models::task::order_tasks_with_store(someday_tasks, store);
            if someday_tasks.is_empty() {
                println!("No someday tasks");
            } else {
                saku_tdo::ui::render_view_header("Someday", someday_tasks.len());
                for task in someday_tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
            }
        }
        ViewEntity::All => {
            use std::collections::HashMap;
            let all_tasks: Vec<_> = store.get_active_tasks().collect();
            let all_tasks = filter_tasks(all_tasks, filters, store);
            let all_tasks = saku_tdo::models::task::order_tasks_with_store(all_tasks, store);
            if all_tasks.is_empty() {
                println!("No active tasks");
            } else {
                let mut grouped: HashMap<String, Vec<&saku_tdo::models::task::Task>> =
                    HashMap::new();
                for task in &all_tasks {
                    let group = match &task.when {
                        When::Inbox => "Inbox",
                        When::Someday => "Someday",
                        When::Scheduled { date: _, .. } => "Scheduled",
                        When::LegacyToday { .. } | When::LegacyAnytime => "Legacy",
                    };
                    grouped.entry(group.to_string()).or_default().push(task);
                }
                let order = vec!["Inbox", "Scheduled", "Someday"];
                for group_name in order {
                    if let Some(tasks) = grouped.get(group_name) {
                        saku_tdo::ui::render_view_header(group_name, tasks.len());
                        for task in tasks {
                            saku_tdo::ui::render_task_line(task, store);
                        }
                    }
                }
            }
        }
        ViewEntity::Upcoming => {
            use jiff::civil::Date;
            use std::collections::BTreeMap;
            let today = jiff::Zoned::now().date();
            let upcoming_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    (include_completed || t.completed_at.is_none()) && {
                        let scheduled_future = match t.when {
                            When::Scheduled { date, .. } => date > today,
                            _ => false,
                        };
                        let deadline_future = t.deadline.is_some_and(|d| d > today);
                        scheduled_future || deadline_future
                    }
                })
                .collect();
            let upcoming_tasks = filter_tasks(upcoming_tasks, filters, store);
            if upcoming_tasks.is_empty() {
                println!("No upcoming tasks");
            } else {
                let mut grouped: BTreeMap<Date, Vec<&saku_tdo::models::task::Task>> =
                    BTreeMap::new();
                for task in &upcoming_tasks {
                    let date = match task.when {
                        When::Scheduled { date, .. } => Some(date),
                        _ => None,
                    };
                    let deadline = task.deadline;
                    let key_date = match (date, deadline) {
                        (Some(d1), Some(d2)) => Some(d1.min(d2)),
                        (Some(d), None) | (None, Some(d)) => Some(d),
                        (None, None) => None,
                    };
                    if let Some(key) = key_date {
                        grouped.entry(key).or_default().push(task);
                    }
                }
                saku_tdo::ui::render_view_header("Upcoming", upcoming_tasks.len());
                for (date, mut tasks) in grouped {
                    tasks.sort_by_key(|t| t.task_number);
                    saku_tdo::ui::render_section_header(&saku_tdo::ui::format_date_header(date));
                    for task in tasks {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
            }
        }
        ViewEntity::Deadlines => {
            let today = jiff::Zoned::now().date();
            let deadline_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| (include_completed || t.completed_at.is_none()) && t.deadline.is_some())
                .collect();
            let mut deadline_tasks = filter_tasks(deadline_tasks, filters, store);
            if deadline_tasks.is_empty() {
                println!("No tasks with deadlines");
            } else {
                deadline_tasks.sort_by(|a, b| {
                    a.deadline
                        .cmp(&b.deadline)
                        .then(a.task_number.cmp(&b.task_number))
                });
                saku_tdo::ui::render_view_header("Deadlines", deadline_tasks.len());
                let end_of_week = {
                    let days_to_sunday = match today.weekday() {
                        jiff::civil::Weekday::Sunday => 0,
                        jiff::civil::Weekday::Monday => 6,
                        jiff::civil::Weekday::Tuesday => 5,
                        jiff::civil::Weekday::Wednesday => 4,
                        jiff::civil::Weekday::Thursday => 3,
                        jiff::civil::Weekday::Friday => 2,
                        jiff::civil::Weekday::Saturday => 1,
                    };
                    today
                        .checked_add(jiff::Span::new().days(days_to_sunday))
                        .expect("valid date")
                };
                let mut overdue: Vec<&saku_tdo::models::task::Task> = Vec::new();
                let mut due_today: Vec<&saku_tdo::models::task::Task> = Vec::new();
                let mut this_week: Vec<&saku_tdo::models::task::Task> = Vec::new();
                let mut later: Vec<&saku_tdo::models::task::Task> = Vec::new();
                for task in &deadline_tasks {
                    let d = task.deadline.unwrap();
                    if d < today {
                        overdue.push(task);
                    } else if d == today {
                        due_today.push(task);
                    } else if d <= end_of_week {
                        this_week.push(task);
                    } else {
                        later.push(task);
                    }
                }
                if !overdue.is_empty() {
                    saku_tdo::ui::render_section_header(&format!("Overdue ({})", overdue.len()));
                    for task in overdue {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
                if !due_today.is_empty() {
                    saku_tdo::ui::render_section_header(&format!("Today ({})", due_today.len()));
                    for task in due_today {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
                if !this_week.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "This Week ({})",
                        this_week.len()
                    ));
                    for task in this_week {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
                if !later.is_empty() {
                    saku_tdo::ui::render_section_header(&format!("Later ({})", later.len()));
                    for task in later {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
            }
        }
        ViewEntity::Logbook => {
            use std::collections::BTreeMap;
            let completed_tasks: Vec<_> = store
                .tasks
                .values()
                .filter(|t| {
                    if let Some(completed_at) = t.completed_at {
                        saku_tdo::ui::is_within_days(completed_at, 14)
                    } else {
                        false
                    }
                })
                .collect();
            if completed_tasks.is_empty() {
                println!("No completed tasks in the last 14 days");
            } else {
                let mut grouped: BTreeMap<(i16, i8), Vec<&saku_tdo::models::task::Task>> =
                    BTreeMap::new();
                for task in &completed_tasks {
                    if let Some(completed_at) = task.completed_at {
                        let year_month = saku_tdo::ui::get_year_month(completed_at);
                        grouped.entry(year_month).or_default().push(task);
                    }
                }
                saku_tdo::ui::render_view_header("Logbook", completed_tasks.len());
                for (_year_month, tasks) in grouped.iter().rev() {
                    let mut sorted_tasks = tasks.clone();
                    sorted_tasks
                        .sort_by(|a, b| b.completed_at.unwrap().cmp(&a.completed_at.unwrap()));
                    let month_header =
                        saku_tdo::ui::format_month_header(sorted_tasks[0].completed_at.unwrap());
                    saku_tdo::ui::render_section_header(&month_header);
                    for task in sorted_tasks {
                        saku_tdo::ui::render_task_line_with_completion_date(task, store);
                    }
                }
            }
        }
        ViewEntity::Trash => {
            let deleted_tasks: Vec<_> = store.get_deleted_tasks().collect();
            let deleted_projects: Vec<_> = store.get_deleted_projects().collect();
            let deleted_areas: Vec<_> = store.get_deleted_areas().collect();
            let total = deleted_tasks.len() + deleted_projects.len() + deleted_areas.len();
            if total == 0 {
                println!("Trash is empty");
            } else {
                saku_tdo::ui::render_view_header("Trash", total);
                if !deleted_tasks.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Tasks ({})",
                        deleted_tasks.len()
                    ));
                    for task in deleted_tasks {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
                if !deleted_projects.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Projects ({})",
                        deleted_projects.len()
                    ));
                    for project in deleted_projects {
                        println!("  {} {}", "•".dimmed(), project.name.dimmed());
                    }
                }
                if !deleted_areas.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Areas ({})",
                        deleted_areas.len()
                    ));
                    for area in deleted_areas {
                        println!("  {} {}", "•".dimmed(), area.name.dimmed());
                    }
                }
            }
        }
        ViewEntity::Project { name } => {
            let matching: Vec<_> = store
                .get_active_projects()
                .filter(|p| p.name.to_lowercase().contains(&name.to_lowercase()))
                .collect();
            let project = match matching.len() {
                0 => {
                    eprintln!("Error: Project '{}' not found", name);
                    let projects: Vec<_> = store.get_active_projects().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for p in projects {
                            eprintln!("  - {}", p.name);
                        }
                    }
                    std::process::exit(1);
                }
                1 => matching[0],
                _ => {
                    eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                    for p in &matching {
                        eprintln!("  - {}", p.name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
            };
            let mut tasks: Vec<_> = store
                .get_tasks_for_project(&project.storage_key())
                .filter(|t| (include_completed || t.completed_at.is_none()) && t.deleted_at.is_none())
                .collect();
            tasks.sort_by_key(|t| t.task_number);
            let header = if let Some(ref area_key) = project.area_key {
                if let Some(area) = store.get_area(area_key) {
                    format!("{} ({})", project.name, area.name)
                } else {
                    project.name.clone()
                }
            } else {
                project.name.clone()
            };
            if tasks.is_empty() {
                println!("No tasks in project '{}'", header);
            } else {
                saku_tdo::ui::render_view_header(&header, tasks.len());
                for task in tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
            }
        }
        ViewEntity::Area { name } => {
            let matching: Vec<_> = store
                .get_active_areas()
                .filter(|a| a.name.to_lowercase().contains(&name.to_lowercase()))
                .collect();
            let area = match matching.len() {
                0 => {
                    eprintln!("Error: Area '{}' not found", name);
                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for a in areas {
                            eprintln!("  - {}", a.name);
                        }
                    }
                    std::process::exit(1);
                }
                1 => matching[0],
                _ => {
                    eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                    for a in &matching {
                        eprintln!("  - {}", a.name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
            };
            let area_key = area.storage_key();
            let mut direct_tasks: Vec<_> = store
                .get_tasks_for_area(&area_key)
                .filter(|t| (include_completed || t.completed_at.is_none()) && t.deleted_at.is_none())
                .collect();
            direct_tasks.sort_by_key(|t| t.task_number);
            let mut projects: Vec<_> = store
                .get_projects_for_area(&area_key)
                .filter(|p| p.deleted_at.is_none())
                .collect();
            projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            let project_tasks: Vec<_> = projects
                .iter()
                .map(|p| {
                    let mut tasks: Vec<_> = store
                        .get_tasks_for_project(&p.storage_key())
                        .filter(|t| (include_completed || t.completed_at.is_none()) && t.deleted_at.is_none())
                        .collect();
                    tasks.sort_by_key(|t| t.task_number);
                    (*p, tasks)
                })
                .filter(|(_, tasks)| !tasks.is_empty())
                .collect();
            let total_tasks = direct_tasks.len()
                + project_tasks
                    .iter()
                    .map(|(_, tasks)| tasks.len())
                    .sum::<usize>();
            if total_tasks == 0 {
                println!("No tasks in area '{}'", area.name);
            } else {
                saku_tdo::ui::render_view_header(&area.name, total_tasks);
                for task in &direct_tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
                for (project, tasks) in project_tasks.iter() {
                    saku_tdo::ui::render_section_header(&project.name);
                    for task in tasks {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
            }
        }
        ViewEntity::Tag { name } => {
            let tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    (include_completed || t.completed_at.is_none())
                        && t.tags
                            .iter()
                            .any(|tag| tag.to_lowercase() == name.to_lowercase())
                })
                .collect();
            let mut tasks = filter_tasks(tasks, filters, store);
            tasks.sort_by_key(|t| t.task_number);
            if tasks.is_empty() {
                println!("No tasks with tag '{}'", name);
                use std::collections::HashSet;
                let available_tags: HashSet<_> = store
                    .get_active_tasks()
                    .filter(|t| include_completed || t.completed_at.is_none())
                    .flat_map(|t| &t.tags)
                    .collect();
                if !available_tags.is_empty() {
                    println!("\nAvailable tags:");
                    for tag in available_tags {
                        println!("  - {}", tag);
                    }
                }
            } else {
                saku_tdo::ui::render_view_header(&format!("#{}", name), tasks.len());
                for task in tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
            }
        }
        ViewEntity::Recurring => {
            let today = jiff::Zoned::now().date();
            let mut tasks: Vec<_> = store.get_recurring_tasks().collect();
            tasks.sort_by_key(|t| t.task_number);

            if tasks.is_empty() {
                println!("No recurring tasks.");
            } else {
                saku_tdo::ui::render_view_header("Recurring", tasks.len());
                for task in tasks {
                    let next = next_pending_occurrence(task, today);
                    if let Some(next_date) = next {
                        saku_tdo::ui::render_task_line_with_next_occurrence(task, store, next_date);
                    } else {
                        saku_tdo::ui::render_task_line(task, store);
                    }
                }
            }
        }
        ViewEntity::Deferred => {
            let today = jiff::Zoned::now().date();
            let deferred_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.completed_at.is_none())
                .filter(|t| t.defer_until.is_some_and(|d| d > today))
                .collect();
            let deferred_tasks = filter_tasks(deferred_tasks, filters, store);
            let mut deferred_tasks = deferred_tasks;
            deferred_tasks.sort_by_key(|t| t.defer_until);
            if deferred_tasks.is_empty() {
                println!("No deferred tasks");
            } else {
                saku_tdo::ui::render_view_header("Deferred", deferred_tasks.len());
                for task in deferred_tasks {
                    saku_tdo::ui::render_task_line(task, store);
                }
            }
        }
        ViewEntity::Task { id } => {
            resolve_task_by_id_or_fuzzy(id, store, |task| {
                saku_tdo::ui::render_task_detail_view(task, store);
            });
        }
    }
}

/// Resolve a task by numeric ID or fuzzy title match, then call `f` with the task.
/// Exits the process on error (not found / ambiguous).
fn resolve_task_by_id_or_fuzzy<F>(id: &str, store: &saku_tdo::models::store::Store, f: F)
where
    F: FnOnce(&saku_tdo::models::task::Task),
{
    if let Ok(n) = id.parse::<u64>() {
        match store.get_task_by_number(n) {
            Some(task) => f(task),
            None => {
                eprintln!("Error: Task #{} not found", n);
                std::process::exit(1);
            }
        }
    } else {
        let lower = id.to_lowercase();
        let matches: Vec<_> = store
            .tasks
            .values()
            .filter(|t| t.title.to_lowercase().contains(&lower))
            .collect();
        match matches.len() {
            0 => {
                eprintln!("Error: No task found matching '{}'", id);
                std::process::exit(1);
            }
            1 => f(matches[0]),
            _ => {
                eprintln!("Error: Ambiguous match for '{}'. Multiple tasks found:", id);
                for t in &matches {
                    eprintln!("  #{} {}", t.task_number, t.title);
                }
                eprintln!("\nPlease be more specific.");
                std::process::exit(1);
            }
        }
    }
}

/// Check if a command is a mutating (write) command that should trigger sync.
fn is_mutating_command(cmd: &Option<Commands>) -> bool {
    matches!(
        cmd,
        Some(
            Commands::Add { .. }
                | Commands::Move { .. }
                | Commands::Done { .. }
                | Commands::Depend { .. }
                | Commands::Delete { .. }
                | Commands::Restore { .. }
                | Commands::Create { .. }
                | Commands::Remove { .. }
                | Commands::Edit { .. }
        )
    )
}

/// Render the today view (used by both `tdo today` and `tdo` with no args).
fn render_today_view(store: &saku_tdo::models::store::Store, include_completed: bool, filters: &ViewFilters) {
    let today = jiff::Zoned::now().date();

    // Collect overdue tasks (scheduled or deadline < today), excluding subtasks.
    // Recurring tasks are never "overdue" based on their scheduled date — their
    // pending-ness is derived entirely from the recurrence rule.
    // Hide deferred tasks (defer_until > today).
    let overdue_tasks: Vec<_> = store
        .get_active_tasks()
        .filter(|t| t.parent_task_key.is_none())
        .filter(|t| t.defer_until.is_none() || t.defer_until.unwrap() <= today)
        .filter(|t| {
            (include_completed || t.completed_at.is_none()) && t.recurrence.is_none() && {
                let scheduled_overdue = match t.when {
                    When::Scheduled { date, .. } => date < today,
                    _ => false,
                };
                let deadline_overdue = t.deadline.is_some_and(|d| d < today);
                scheduled_overdue || deadline_overdue
            }
        })
        .collect();
    let overdue_tasks = filter_tasks(overdue_tasks, filters, store);
    let overdue_tasks = saku_tdo::models::task::order_tasks_with_store(overdue_tasks, store);

    // Collect today tasks (scheduled or deadline == today, or a recurring occurrence today), excluding subtasks
    // Hide deferred tasks (defer_until > today).
    let today_current: Vec<_> = store
        .get_active_tasks()
        .filter(|t| t.parent_task_key.is_none())
        .filter(|t| t.defer_until.is_none() || t.defer_until.unwrap() <= today)
        .filter(|t| {
            (include_completed || t.completed_at.is_none()) && {
                let scheduled_today = match t.when {
                    When::Scheduled { date, .. } => date == today,
                    _ => false,
                };
                let deadline_today = t.deadline == Some(today);
                let recurring_today = is_pending_on(t, today);

                // For non-recurring tasks, exclude those already in the overdue bucket.
                // Recurring tasks are never moved to overdue, so always include them when pending.
                if recurring_today {
                    return true;
                }
                let is_overdue = match t.when {
                    When::Scheduled { date, .. } if date < today => true,
                    _ => t.deadline.is_some_and(|d| d < today),
                };

                (scheduled_today || deadline_today) && !is_overdue
            }
        })
        .collect();
    let today_current = filter_tasks(today_current, filters, store);
    let today_current = saku_tdo::models::task::order_tasks_with_store(today_current, store);

    let total = overdue_tasks.len() + today_current.len();

    if total == 0 {
        println!("No tasks for today");
    } else {
        saku_tdo::ui::render_view_header(&format!("Today ({})", today.strftime("%b %d")), total);

        // Show overdue first if any
        if !overdue_tasks.is_empty() {
            saku_tdo::ui::render_section_header("Overdue");
            for task in overdue_tasks {
                saku_tdo::ui::render_task_line(task, store);
                saku_tdo::ui::render_subtask_children(&task.storage_key(), store);
            }
        }

        // Show today tasks
        if !today_current.is_empty() {
            for task in today_current {
                saku_tdo::ui::render_task_line(task, store);
                saku_tdo::ui::render_subtask_children(&task.storage_key(), store);
            }
        }
    }
}

/// Render the pretty-print output for `tdo context`.
fn render_context_pretty(
    ctx: &output::ContextOutput,
    store: &saku_tdo::models::store::Store,
    today: jiff::civil::Date,
) {
    use colored::*;

    let date_label = today.strftime("%b %d").to_string();

    // --- Summary header (custom, no count suffix) ---
    let header_title = format!("Context · {}", date_label);
    println!(
        "\n  {} {}\n",
        "▌".cyan().bold(),
        header_title.cyan().bold()
    );

    // Summary label-value pairs
    let today_detail = if ctx.today.total > 0 {
        format!(
            "{} tasks ({} ready, {} blocked)",
            ctx.today.total, ctx.today.ready, ctx.today.blocked
        )
    } else {
        "No tasks".to_string()
    };
    println!("    {:<12}{}", "Today".bold(), today_detail);

    if ctx.inbox_count > 0 {
        let inbox_detail = if ctx.needs_review_count > 0 {
            format!(
                "{} items ({} need review)",
                ctx.inbox_count, ctx.needs_review_count
            )
        } else {
            format!("{} items", ctx.inbox_count)
        };
        println!("    {:<12}{}", "Inbox".bold(), inbox_detail);
    }

    if !ctx.overdue_tasks.is_empty() {
        let task_word = if ctx.overdue_tasks.len() == 1 {
            "task"
        } else {
            "tasks"
        };
        println!(
            "    {:<12}{}",
            "Overdue".bold(),
            format!("{} {}", ctx.overdue_tasks.len(), task_word).red()
        );
    }

    // --- Ready Now section ---
    if !ctx.ready_tasks.is_empty() {
        saku_tdo::ui::render_view_header("Ready Now", ctx.ready_tasks.len());
        for task_out in &ctx.ready_tasks {
            if let Some(task) = store.get_task_by_number(task_out.id) {
                saku_tdo::ui::render_task_line(task, store);
            }
        }
    }

    // --- Blocked section ---
    if !ctx.blocked_tasks.is_empty() {
        saku_tdo::ui::render_view_header("Blocked", ctx.blocked_tasks.len());
        for bt in &ctx.blocked_tasks {
            if let Some(task) = store.get_task_by_number(bt.task_number) {
                saku_tdo::ui::render_task_line(task, store);
            }
        }
    }

    // --- Overdue section ---
    if !ctx.overdue_tasks.is_empty() {
        saku_tdo::ui::render_view_header("Overdue", ctx.overdue_tasks.len());
        for task_out in &ctx.overdue_tasks {
            if let Some(task) = store.get_task_by_number(task_out.id) {
                saku_tdo::ui::render_task_line(task, store);
            }
        }
    }

    // --- Recent Completions section ---
    if !ctx.recent_completions.is_empty() {
        saku_tdo::ui::render_view_header("Recent Completions (48h)", ctx.recent_completions.len());
        for task_out in &ctx.recent_completions {
            if let Some(task) = store.get_task_by_number(task_out.id) {
                saku_tdo::ui::render_task_line_with_completion_date(task, store);
            }
        }
    }

    // --- Active Projects section ---
    if !ctx.active_projects.is_empty() {
        saku_tdo::ui::render_view_header("Active Projects", ctx.active_projects.len());
        let project_list: Vec<String> = ctx
            .active_projects
            .iter()
            .map(|p| format!("{} ({})", p.name, p.task_count))
            .collect();
        println!("    {}", project_list.join(", "));
    }

    println!();
}

/// Attempt to sync after a mutation.
/// Tries server sync first (if configured), falls back to TDO_SYNC_DIR for local dev.
/// Sync is best-effort: errors are printed as warnings but never abort.
fn try_sync(storage_path: &std::path::Path) {
    // Allow tests (and users) to fully disable sync without keychain prompts
    if std::env::var_os("TDO_NO_SYNC").is_some() {
        return;
    }

    // Try server sync first (if the sync feature is enabled and configured)
    #[cfg(feature = "sync")]
    {
        if let Ok(Some(config)) = saku_sync::config::load_sync_config() {
            // Read passphrase from keychain
            match saku_crypto::keychain::SyncCredentialStore::new()
                .and_then(|s| s.load_or_migrate())
                .ok()
                .and_then(|c| c.passphrase)
            {
                Some(passphrase) => {
                    match saku_sync::try_flush_if_online_server(
                        storage_path,
                        passphrase.as_bytes(),
                        &config.server_url,
                        &config.device_id,
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Warning: sync failed: {}", e);
                        }
                    }
                    return;
                }
                None => {
                    // No passphrase in keychain, fall through to local sync
                }
            }
        }
    }

    // Fallback: local filesystem sync via TDO_SYNC_DIR env var
    if let Some(sync_dir) = std::env::var_os("TDO_SYNC_DIR") {
        let sync_dir = PathBuf::from(sync_dir);
        let passphrase = b"saku-dev-passphrase";
        match saku_sync::try_flush_if_online(storage_path, passphrase, &sync_dir) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Warning: sync failed: {}", e);
            }
        }
    }
}

fn main() {
    // Initialize logging if feature is enabled
    #[cfg(feature = "logging")]
    {
        if let Err(e) = logging::init() {
            eprintln!("Warning: Failed to initialize logging: {}", e);
        }
    }

    // Create root span for the entire application
    #[cfg(feature = "logging")]
    let _span = tracing::info_span!("tdo_app", version = env!("CARGO_PKG_VERSION")).entered();

    let cli = Cli::parse();

    // Check if this is a mutating command (for post-mutation sync)
    let should_sync = is_mutating_command(&cli.command);

    // Ensure device_id exists (created on first run, used by sync in the future)
    if let Err(e) = saku_storage::device::get_or_create_device_id() {
        eprintln!("Warning: Failed to initialize device ID: {}", e);
    }

    // Initialize storage
    let storage_path = std::env::var_os("TDO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tdo")
        })
        .join("store.json");

    // Create parent directory if it doesn't exist
    if let Some(parent) = storage_path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("Error: Failed to create data directory: {}", e);
            std::process::exit(1);
        });
    }

    let storage = JsonFileStorage::new(storage_path.clone());

    // Sync BEFORE loading store (unless this is a sync management command)
    #[cfg(feature = "sync")]
    let is_sync_command = matches!(cli.command, Some(Commands::Sync { .. }));
    #[cfg(not(feature = "sync"))]
    let is_sync_command = false;
    if !is_sync_command {
        try_sync(&storage_path);
    }

    let mut store = match storage.load() {
        Ok(store) => store,
        Err(e) => {
            eprintln!("Error: Failed to load store: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Today) => {
            eprintln!(
                "{}",
                "Warning: 'tdo today' is deprecated. Use 'tdo view today' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();
            let no_filters = ViewFilters { project: None, tags: vec![], area: None, ready: false };
            render_today_view(&store, false, &no_filters);
        }
        Some(Commands::Inbox) => {
            eprintln!(
                "{}",
                "Warning: 'tdo inbox' is deprecated. Use 'tdo view inbox' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            // Filter inbox tasks (excluding subtasks)
            let inbox_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| matches!(t.when, When::Inbox))
                .filter(|t| t.completed_at.is_none())
                .collect();
            let inbox_tasks = saku_tdo::models::task::order_tasks_with_store(inbox_tasks, &store);

            // Display
            if inbox_tasks.is_empty() {
                println!("Inbox is empty");
            } else {
                saku_tdo::ui::render_view_header("Inbox", inbox_tasks.len());
                for task in inbox_tasks {
                    saku_tdo::ui::render_task_line(task, &store);
                    saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                }
            }
        }
        Some(Commands::Someday) => {
            eprintln!(
                "{}",
                "Warning: 'tdo someday' is deprecated. Use 'tdo view someday' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            // Filter someday tasks (excluding subtasks)
            let someday_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| matches!(t.when, When::Someday))
                .filter(|t| t.completed_at.is_none())
                .collect();
            let someday_tasks = saku_tdo::models::task::order_tasks_with_store(someday_tasks, &store);

            // Display
            if someday_tasks.is_empty() {
                println!("No someday tasks");
            } else {
                saku_tdo::ui::render_view_header("Someday", someday_tasks.len());
                for task in someday_tasks {
                    saku_tdo::ui::render_task_line(task, &store);
                    saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                }
            }
        }
        Some(Commands::All) => {
            eprintln!(
                "{}",
                "Warning: 'tdo all' is deprecated. Use 'tdo view all' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            use std::collections::HashMap;

            // Collect all active, incomplete tasks (excluding subtasks)
            let all_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .collect();
            let all_tasks = saku_tdo::models::task::order_tasks_with_store(all_tasks, &store);

            if all_tasks.is_empty() {
                println!("No active tasks");
            } else {
                // Group tasks by When variant
                let mut grouped: HashMap<String, Vec<&saku_tdo::models::task::Task>> =
                    HashMap::new();

                for task in &all_tasks {
                    let group = match &task.when {
                        When::Inbox => "Inbox",
                        When::Someday => "Someday",
                        When::Scheduled { date: _, .. } => "Scheduled",
                        When::LegacyToday { .. } | When::LegacyAnytime => "Legacy", // Should not appear after migration
                    };
                    grouped.entry(group.to_string()).or_default().push(task);
                }

                // Display in a logical order
                let order = vec!["Inbox", "Scheduled", "Someday"];

                for group_name in order {
                    if let Some(tasks) = grouped.get(group_name) {
                        saku_tdo::ui::render_view_header(group_name, tasks.len());
                        for task in tasks {
                            saku_tdo::ui::render_task_line(task, &store);
                            saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                        }
                    }
                }
            }
        }
        Some(Commands::Upcoming) => {
            eprintln!(
                "{}",
                "Warning: 'tdo upcoming' is deprecated. Use 'tdo view upcoming' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            use jiff::civil::Date;
            use std::collections::BTreeMap;

            let today = jiff::Zoned::now().date();

            // Collect upcoming tasks (scheduled or deadline in the future), excluding subtasks
            let upcoming_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| {
                    t.completed_at.is_none() && {
                        let scheduled_future = match t.when {
                            When::Scheduled { date, .. } => date > today,
                            _ => false,
                        };
                        let deadline_future = t.deadline.is_some_and(|d| d > today);
                        scheduled_future || deadline_future
                    }
                })
                .collect();

            if upcoming_tasks.is_empty() {
                println!("No upcoming tasks");
            } else {
                // Group by date (use earliest of scheduled or deadline)
                let mut grouped: BTreeMap<Date, Vec<&saku_tdo::models::task::Task>> =
                    BTreeMap::new();

                for task in &upcoming_tasks {
                    let date = match task.when {
                        When::Scheduled { date, .. } => Some(date),
                        _ => None,
                    };
                    let deadline = task.deadline;

                    // Use earliest date
                    let key_date = match (date, deadline) {
                        (Some(d1), Some(d2)) => Some(d1.min(d2)),
                        (Some(d), None) | (None, Some(d)) => Some(d),
                        (None, None) => None,
                    };

                    if let Some(key) = key_date {
                        grouped.entry(key).or_default().push(task);
                    }
                }

                saku_tdo::ui::render_view_header("Upcoming", upcoming_tasks.len());

                // Display by date
                for (date, mut tasks) in grouped {
                    tasks.sort_by_key(|t| t.task_number);
                    saku_tdo::ui::render_section_header(&saku_tdo::ui::format_date_header(date));
                    for task in tasks {
                        saku_tdo::ui::render_task_line(task, &store);
                        saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                    }
                }
            }
        }
        Some(Commands::Logbook) => {
            eprintln!(
                "{}",
                "Warning: 'tdo logbook' is deprecated. Use 'tdo view logbook' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            use std::collections::BTreeMap;

            // Collect completed top-level tasks from last 14 days (excluding subtasks)
            let completed_tasks: Vec<_> = store
                .tasks
                .values()
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| {
                    if let Some(completed_at) = t.completed_at {
                        saku_tdo::ui::is_within_days(completed_at, 14)
                    } else {
                        false
                    }
                })
                .collect();

            if completed_tasks.is_empty() {
                println!("No completed tasks in the last 14 days");
            } else {
                // Group by month
                let mut grouped: BTreeMap<(i16, i8), Vec<&saku_tdo::models::task::Task>> =
                    BTreeMap::new();

                for task in &completed_tasks {
                    if let Some(completed_at) = task.completed_at {
                        let year_month = saku_tdo::ui::get_year_month(completed_at);
                        grouped.entry(year_month).or_default().push(task);
                    }
                }

                saku_tdo::ui::render_view_header("Logbook", completed_tasks.len());

                // Display by month (most recent first)
                for (_year_month, tasks) in grouped.iter().rev() {
                    // Sort tasks within month by completion time (most recent first)
                    let mut sorted_tasks = tasks.clone();
                    sorted_tasks
                        .sort_by(|a, b| b.completed_at.unwrap().cmp(&a.completed_at.unwrap()));

                    // Use the first task's timestamp to format the month header
                    let month_header =
                        saku_tdo::ui::format_month_header(sorted_tasks[0].completed_at.unwrap());
                    saku_tdo::ui::render_section_header(&month_header);

                    for task in sorted_tasks {
                        saku_tdo::ui::render_task_line_with_completion_date(task, &store);
                        saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                    }
                }
            }
        }
        Some(Commands::Trash) => {
            eprintln!(
                "{}",
                "Warning: 'tdo trash' is deprecated. Use 'tdo view trash' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            // Collect deleted items (exclude cascade-deleted subtasks from top-level list)
            let deleted_tasks: Vec<_> = store
                .get_deleted_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .collect();
            let deleted_projects: Vec<_> = store.get_deleted_projects().collect();
            let deleted_areas: Vec<_> = store.get_deleted_areas().collect();

            let total = deleted_tasks.len() + deleted_projects.len() + deleted_areas.len();

            if total == 0 {
                println!("Trash is empty");
            } else {
                saku_tdo::ui::render_view_header("Trash", total);

                // Show deleted tasks
                if !deleted_tasks.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Tasks ({})",
                        deleted_tasks.len()
                    ));
                    for task in deleted_tasks {
                        saku_tdo::ui::render_task_line(task, &store);
                    }
                }

                // Show deleted projects
                if !deleted_projects.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Projects ({})",
                        deleted_projects.len()
                    ));
                    for project in deleted_projects {
                        println!("  {} {}", "•".dimmed(), project.name.dimmed());
                    }
                }

                // Show deleted areas
                if !deleted_areas.is_empty() {
                    saku_tdo::ui::render_section_header(&format!(
                        "Areas ({})",
                        deleted_areas.len()
                    ));
                    for area in deleted_areas {
                        println!("  {} {}", "•".dimmed(), area.name.dimmed());
                    }
                }
            }
        }
        Some(Commands::Search {
            query,
            notes,
            json,
            csv,
        }) => {
            let results = store.search_tasks(&query, notes);
            let fmt = output::OutputFormat::from_flags(json, csv);
            match fmt {
                output::OutputFormat::Pretty => {
                    if results.is_empty() {
                        println!("No tasks matching \"{}\"", query);
                    } else {
                        saku_tdo::ui::render_view_header("Search results", results.len());
                        for task in results {
                            saku_tdo::ui::render_task_line(task, &store);
                        }
                    }
                }
                output::OutputFormat::Json => {
                    let out: Vec<_> = results
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    output::print_json(&out);
                }
                output::OutputFormat::Csv => {
                    let out: Vec<_> = results
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    output::print_csv(&out);
                }
            }
        }
        Some(Commands::Context { json, csv }) => {
            let today = jiff::Zoned::now().date();
            let ctx = output::build_context(&store, today);
            let fmt = output::OutputFormat::from_flags(json, csv);
            match fmt {
                output::OutputFormat::Json => {
                    output::print_json(&ctx);
                }
                output::OutputFormat::Csv => {
                    eprintln!("CSV output is not supported for context. Use --json instead.");
                    std::process::exit(1);
                }
                output::OutputFormat::Pretty => {
                    render_context_pretty(&ctx, &store, today);
                }
            }
        }
        Some(Commands::View {
            entity: entity_str,
            name: entity_name,
            json,
            csv,
            watch,
            all,
            project: filter_project,
            tag: filter_tags,
            area: filter_area,
            ready: filter_ready,
        }) => {
            let entity = match parse_view_entity(&entity_str, entity_name) {
                Ok(e) => e,
                Err(msg) => {
                    eprintln!("Error: {}", msg);
                    std::process::exit(1);
                }
            };
            let view_filters = ViewFilters {
                project: filter_project,
                tags: filter_tags,
                area: filter_area,
                ready: filter_ready,
            };
            if watch {
                loop {
                    // Clear screen and move cursor to top-left
                    print!("\x1B[2J\x1B[H");
                    {
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    let store = match storage.load() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Error: Failed to reload store: {}", e);
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            continue;
                        }
                    };
                    render_view_pretty(&entity, &store, all, &view_filters);
                    let now = jiff::Zoned::now();
                    let width = term_size::dimensions().map(|(w, _)| w).unwrap_or(40);
                    println!();
                    println!("{}", "─".repeat(width).dimmed());
                    println!(
                        "{}",
                        format!(
                            "Watching · {} · Ctrl+C to quit",
                            now.strftime("%H:%M:%S")
                        )
                        .dimmed()
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
            let fmt = output::OutputFormat::from_flags(json, csv);
            if matches!(fmt, output::OutputFormat::Pretty) {
                render_view_pretty(&entity, &store, all, &view_filters);
            } else {
            match entity {
                ViewEntity::Today => {
                        let today = jiff::Zoned::now().date();
                        let tasks: Vec<_> = store
                            .get_active_tasks()
                            .filter(|t| {
                                (all || t.completed_at.is_none()) && {
                                    let scheduled_today_or_overdue = match t.when {
                                        When::Scheduled { date, .. } => date <= today,
                                        _ => false,
                                    };
                                    let deadline_today_or_overdue =
                                        t.deadline.is_some_and(|d| d <= today);
                                    scheduled_today_or_overdue || deadline_today_or_overdue
                                }
                            })
                            .collect();
                        let tasks = filter_tasks(tasks, &view_filters, &store);
                        let out: Vec<_> = tasks
                            .iter()
                            .map(|t| output::TaskOutput::from_task(t, &store))
                            .collect();
                        match fmt {
                            output::OutputFormat::Json => output::print_json(&out),
                            output::OutputFormat::Csv => output::print_csv(&out),
                            output::OutputFormat::Pretty => unreachable!(),
                        }
                }
                ViewEntity::Inbox => {
                    let inbox_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| matches!(t.when, When::Inbox))
                        .filter(|t| all || t.completed_at.is_none())
                        .collect();
                    let inbox_tasks = filter_tasks(inbox_tasks, &view_filters, &store);
                        let out: Vec<_> = inbox_tasks
                            .iter()
                            .map(|t| output::TaskOutput::from_task(t, &store))
                            .collect();
                        match fmt {
                            output::OutputFormat::Json => output::print_json(&out),
                            output::OutputFormat::Csv => output::print_csv(&out),
                            output::OutputFormat::Pretty => unreachable!(),
                        }
                }
                ViewEntity::Someday => {
                    let someday_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| matches!(t.when, When::Someday))
                        .filter(|t| all || t.completed_at.is_none())
                        .collect();
                    let someday_tasks = filter_tasks(someday_tasks, &view_filters, &store);
                    let out: Vec<_> = someday_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::All => {
                    let all_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .collect();
                    let all_tasks = filter_tasks(all_tasks, &view_filters, &store);
                    let out: Vec<_> = all_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Upcoming => {
                    let today = jiff::Zoned::now().date();
                    let upcoming_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| {
                            (all || t.completed_at.is_none()) && {
                                let scheduled_future = match t.when {
                                    When::Scheduled { date, .. } => date > today,
                                    _ => false,
                                };
                                let deadline_future = t.deadline.is_some_and(|d| d > today);
                                scheduled_future || deadline_future
                            }
                        })
                        .collect();
                    let upcoming_tasks = filter_tasks(upcoming_tasks, &view_filters, &store);
                    let out: Vec<_> = upcoming_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Deadlines => {
                    let deadline_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| (all || t.completed_at.is_none()) && t.deadline.is_some())
                        .collect();
                    let deadline_tasks = filter_tasks(deadline_tasks, &view_filters, &store);
                    let out: Vec<_> = deadline_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Logbook => {
                    let completed_tasks: Vec<_> = store
                        .tasks
                        .values()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| {
                            if let Some(completed_at) = t.completed_at {
                                saku_tdo::ui::is_within_days(completed_at, 14)
                            } else {
                                false
                            }
                        })
                        .collect();
                    let out: Vec<_> = completed_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Trash => {
                    let deleted_tasks: Vec<_> = store
                        .get_deleted_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .collect();
                    let deleted_projects: Vec<_> = store.get_deleted_projects().collect();
                    let deleted_areas: Vec<_> = store.get_deleted_areas().collect();

                    match fmt {
                        output::OutputFormat::Json => {
                            let trash = output::TrashOutput {
                                tasks: deleted_tasks
                                    .iter()
                                    .map(|t| output::TaskOutput::from_task(t, &store))
                                    .collect(),
                                projects: deleted_projects
                                    .iter()
                                    .map(|p| output::TrashProjectOutput {
                                        name: p.name.clone(),
                                    })
                                    .collect(),
                                areas: deleted_areas
                                    .iter()
                                    .map(|a| output::TrashAreaOutput {
                                        name: a.name.clone(),
                                    })
                                    .collect(),
                            };
                            output::print_json(&trash);
                        }
                        output::OutputFormat::Csv => {
                            let out: Vec<_> = deleted_tasks
                                .iter()
                                .map(|t| output::TaskOutput::from_task(t, &store))
                                .collect();
                            output::print_csv(&out);
                        }
                        output::OutputFormat::Pretty => {
                            let total = deleted_tasks.len()
                                + deleted_projects.len()
                                + deleted_areas.len();
                            if total == 0 {
                                println!("Trash is empty");
                            } else {
                                saku_tdo::ui::render_view_header("Trash", total);
                                if !deleted_tasks.is_empty() {
                                    saku_tdo::ui::render_section_header(&format!(
                                        "Tasks ({})",
                                        deleted_tasks.len()
                                    ));
                                    for task in deleted_tasks {
                                        saku_tdo::ui::render_task_line(task, &store);
                                    }
                                }
                                if !deleted_projects.is_empty() {
                                    saku_tdo::ui::render_section_header(&format!(
                                        "Projects ({})",
                                        deleted_projects.len()
                                    ));
                                    for project in deleted_projects {
                                        println!("  {} {}", "•".dimmed(), project.name.dimmed());
                                    }
                                }
                                if !deleted_areas.is_empty() {
                                    saku_tdo::ui::render_section_header(&format!(
                                        "Areas ({})",
                                        deleted_areas.len()
                                    ));
                                    for area in deleted_areas {
                                        println!("  {} {}", "•".dimmed(), area.name.dimmed());
                                    }
                                }
                            }
                        }
                    }
                }
                ViewEntity::Recurring => {
                    let tasks: Vec<_> = store.get_recurring_tasks().collect();
                    let tasks_filtered = filter_tasks(tasks, &view_filters, &store);
                    let out: Vec<_> = tasks_filtered
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Project { name } => {
                    let matching: Vec<_> = store
                        .get_active_projects()
                        .filter(|p| p.name.to_lowercase().contains(&name.to_lowercase()))
                        .collect();

                    let project = match matching.len() {
                        0 => {
                            eprintln!("Error: Project '{}' not found", name);
                            let projects: Vec<_> = store.get_active_projects().collect();
                            if !projects.is_empty() {
                                eprintln!("\nAvailable projects:");
                                for p in projects {
                                    eprintln!("  - {}", p.name);
                                }
                            }
                            std::process::exit(1);
                        }
                        1 => matching[0],
                        _ => {
                            eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                            for p in &matching {
                                eprintln!("  - {}", p.name);
                            }
                            eprintln!("\nPlease be more specific.");
                            std::process::exit(1);
                        }
                    };

                    let tasks: Vec<_> = store
                        .get_tasks_for_project(&project.storage_key())
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| (all || t.completed_at.is_none()) && t.deleted_at.is_none())
                        .collect();
                    let mut tasks = filter_tasks(tasks, &view_filters, &store);
                    tasks.sort_by_key(|t| t.task_number);

                    match fmt {
                        output::OutputFormat::Json => {
                            let out: Vec<_> = tasks
                                .iter()
                                .map(|t| output::TaskOutput::from_task(t, &store))
                                .collect();
                            output::print_json(&out);
                        }
                        output::OutputFormat::Csv => {
                            let out: Vec<_> = tasks
                                .iter()
                                .map(|t| output::TaskOutput::from_task(t, &store))
                                .collect();
                            output::print_csv(&out);
                        }
                        output::OutputFormat::Pretty => {
                            let header = if let Some(ref area_key) = project.area_key {
                                if let Some(area) = store.get_area(area_key) {
                                    format!("{} ({})", project.name, area.name)
                                } else {
                                    project.name.clone()
                                }
                            } else {
                                project.name.clone()
                            };
                            if tasks.is_empty() {
                                println!("No tasks in project '{}'", header);
                            } else {
                                saku_tdo::ui::render_view_header(&header, tasks.len());
                                for task in tasks {
                                    saku_tdo::ui::render_task_line(task, &store);
                                    saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                                }
                            }
                        }
                    }
                }
                ViewEntity::Area { name } => {
                    let matching: Vec<_> = store
                        .get_active_areas()
                        .filter(|a| a.name.to_lowercase().contains(&name.to_lowercase()))
                        .collect();

                    let area = match matching.len() {
                        0 => {
                            eprintln!("Error: Area '{}' not found", name);
                            let areas: Vec<_> = store.get_active_areas().collect();
                            if !areas.is_empty() {
                                eprintln!("\nAvailable areas:");
                                for a in areas {
                                    eprintln!("  - {}", a.name);
                                }
                            }
                            std::process::exit(1);
                        }
                        1 => matching[0],
                        _ => {
                            eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                            for a in &matching {
                                eprintln!("  - {}", a.name);
                            }
                            eprintln!("\nPlease be more specific.");
                            std::process::exit(1);
                        }
                    };

                    let area_key = area.storage_key();
                    let mut direct_tasks: Vec<_> = store
                        .get_tasks_for_area(&area_key)
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| (all || t.completed_at.is_none()) && t.deleted_at.is_none())
                        .collect();
                    direct_tasks.sort_by_key(|t| t.task_number);

                    let mut projects: Vec<_> = store
                        .get_projects_for_area(&area_key)
                        .filter(|p| p.deleted_at.is_none())
                        .collect();
                    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    let project_tasks: Vec<_> = projects
                        .iter()
                        .map(|p| {
                            let mut tasks: Vec<_> = store
                                .get_tasks_for_project(&p.storage_key())
                                .filter(|t| t.parent_task_key.is_none())
                                .filter(|t| (all || t.completed_at.is_none()) && t.deleted_at.is_none())
                                .collect();
                            tasks.sort_by_key(|t| t.task_number);
                            (*p, tasks)
                        })
                        .filter(|(_, tasks)| !tasks.is_empty())
                        .collect();

                    let all_area_tasks: Vec<&saku_tdo::models::task::Task> = direct_tasks
                        .iter()
                        .chain(
                            project_tasks
                                .iter()
                                .flat_map(|(_, tasks)| tasks.iter()),
                        )
                        .copied()
                        .collect();
                    let all_area_tasks = filter_tasks(all_area_tasks, &view_filters, &store);
                    let out: Vec<_> = all_area_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Tag { name } => {
                    let tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| {
                            (all || t.completed_at.is_none())
                                && t.tags
                                    .iter()
                                    .any(|tag| tag.to_lowercase() == name.to_lowercase())
                        })
                        .collect();
                    let mut tasks = filter_tasks(tasks, &view_filters, &store);
                    tasks.sort_by_key(|t| t.task_number);
                    let out: Vec<_> = tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Deferred => {
                    let today = jiff::Zoned::now().date();
                    let deferred_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.completed_at.is_none())
                        .filter(|t| t.defer_until.is_some_and(|d| d > today))
                        .collect();
                    let deferred_tasks = filter_tasks(deferred_tasks, &view_filters, &store);
                    let out: Vec<_> = deferred_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                ViewEntity::Task { id } => {
                    resolve_task_by_id_or_fuzzy(&id, &store, |task| {
                        let out = output::TaskOutput::from_task(task, &store);
                        match fmt {
                            output::OutputFormat::Json => output::print_json(&out),
                            output::OutputFormat::Csv => output::print_csv(&[out]),
                            output::OutputFormat::Pretty => unreachable!(),
                        }
                    });
                }
            }
            } // close else { match entity { ... } }
        }
        Some(Commands::Add {
            title,
            today,
            tomorrow,
            next_week,
            someday,
            on,
            due,
            project,
            area,
            tag,
            notes,
            defer_until,
            every,
            until,
            parent,
        }) => {
            // Parse when flags
            let when = match When::from_command_flags(today, tomorrow, next_week, someday, on) {
                Ok(w) => w,
                Err(WhenInstantiationError::ScheduleAtIncorrect(date_str, error)) => {
                    eprintln!("Error: Invalid schedule date '{}': {}", date_str, error);
                    std::process::exit(1);
                }
                Err(WhenInstantiationError::ConflictingFlags(flags)) => {
                    eprintln!("Error: Cannot use multiple scheduling flags together");
                    eprintln!("\nConflicting flags provided: {}", flags.join(", "));
                    eprintln!("\nPlease use only one of:");
                    eprintln!("  --today       Schedule for today");
                    eprintln!("  --tomorrow    Schedule for tomorrow");
                    eprintln!("  --next-week   Schedule for next Monday");
                    eprintln!("  --someday     Defer to someday");
                    eprintln!("  --on DATE     Schedule for a specific date");
                    std::process::exit(1);
                }
            };

            // Parse recurrence
            let recurrence = if let Some(pattern) = every {
                let dtstart = match &when {
                    When::Scheduled { date } => *date,
                    _ => jiff::Zoned::now().date(),
                };
                let mut r = match parse_recurrence(&pattern, dtstart) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Some(until_str) = until {
                    use saku_tdo::date_parser::parse_natural_date;
                    match parse_natural_date(&until_str) {
                        Ok(d) => r.until = Some(d),
                        Err(e) => {
                            eprintln!("Error: Invalid --until date '{}': {}", until_str, e);
                            std::process::exit(1);
                        }
                    }
                }
                Some(r)
            } else {
                None
            };

            // Build parameters
            let params = AddTaskParameters {
                title,
                notes,
                when,
                deadline: due,
                defer_until,
                project,
                area,
                tags: tag,
                recurrence,
                parent_task_number: parent,
            };

            // Call service
            match add_task(&mut store, &storage, params) {
                Ok(task) => {
                    println!("✓ Task added: {}", task.title);
                    println!("  #{}", task.task_number);
                    if let Some(ref project_key) = task.project_key
                        && let Some(project) = store.get_project(project_key)
                    {
                        println!("  Project: {}", project.name);
                    }
                    if let Some(ref r) = task.recurrence {
                        println!("  ↻ Repeats: {}", r);
                    }
                }
                Err(AddTaskError::ProjectNotFound(name)) => {
                    eprintln!("Error: Project '{}' not found", name);

                    // Suggest existing projects if any
                    let projects: Vec<_> = store.projects.values().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for project in projects {
                            eprintln!("  - {}", project.name);
                        }
                    } else {
                        eprintln!("\nNo projects exist yet. Create one first or omit --project.");
                    }
                    std::process::exit(1);
                }
                Err(AddTaskError::AmbiguousProjectName(names)) => {
                    eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(AddTaskError::AreaNotFound(name)) => {
                    eprintln!("Error: Area '{}' not found", name);

                    // Suggest existing areas if any
                    let areas: Vec<_> = store.areas.values().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for area in areas {
                            eprintln!("  - {}", area.name);
                        }
                    } else {
                        eprintln!("\nNo areas exist yet. Create one first or omit --area.");
                    }
                    std::process::exit(1);
                }
                Err(AddTaskError::AmbiguousAreaName(names)) => {
                    eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(AddTaskError::InvalidDeadline(date_str, error)) => {
                    eprintln!("Error: Invalid deadline '{}': {}", date_str, error);
                    std::process::exit(1);
                }
                Err(AddTaskError::ParentTaskNotFound(n)) => {
                    eprintln!("Error: Parent task #{} not found", n);
                    std::process::exit(1);
                }
                Err(AddTaskError::ParentIsSubtask) => {
                    eprintln!("Error: Cannot create a subtask of a subtask (only one level of nesting is allowed)");
                    std::process::exit(1);
                }
                Err(AddTaskError::SubtaskCannotHaveProjectOrArea) => {
                    eprintln!("Error: Subtasks inherit project and area from their parent; --project and --area cannot be used with --parent");
                    std::process::exit(1);
                }
                Err(AddTaskError::Storage(e)) => {
                    eprintln!("Error: Failed to save task: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Done {
            task_numbers_or_fuzzy_names,
            stop,
            note,
        }) => {
            for task_number_or_fuzzy_name in task_numbers_or_fuzzy_names {
                // Build parameters
                let params = CompleteTaskParameters {
                    task_number_or_fuzzy_name,
                    stop,
                    note: note.clone(),
                };

                // Call service
                match complete_task(&mut store, &storage, params) {
                    Ok(result) => {
                        if result.task.recurrence.is_some() && !stop {
                            println!("↻ Occurrence marked done: {}", result.task.title);
                            println!("  #{}", result.task.task_number);
                        } else {
                            println!("✓ Task completed: {}", result.task.title);
                            println!("  #{}", result.task.task_number);
                        }
                        if !result.newly_unblocked.is_empty() {
                            println!();
                            for unblocked in &result.newly_unblocked {
                                println!(
                                    "  {} #{} {} is now unblocked",
                                    "→".green(),
                                    unblocked.task_number,
                                    unblocked.title
                                );
                            }
                        }
                    }
                    Err(CompleteTaskError::TaskNotFound(identifier)) => {
                        eprintln!("Error: Task '{}' not found", identifier);
                        std::process::exit(1);
                    }
                    Err(CompleteTaskError::AmbiguousTaskName(titles)) => {
                        eprintln!("Error: Task name is ambiguous. Multiple tasks found:");
                        for title in titles {
                            eprintln!("  - {}", title);
                        }
                        eprintln!("\nPlease be more specific or use the task number.");
                        std::process::exit(1);
                    }
                    Err(CompleteTaskError::HasIncompleteSubtasks) => {
                        eprintln!("Error: Cannot complete task: complete all subtasks first.");
                        std::process::exit(1);
                    }
                    Err(CompleteTaskError::Storage(e)) => {
                        eprintln!("Error: Failed to save task: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Some(Commands::Depend {
            task_number,
            on,
            remove,
        }) => {
            if on.is_none() && remove.is_none() {
                eprintln!("Error: Specify --on <task> to add a dependency or --remove <task> to remove one.");
                std::process::exit(1);
            }

            let params = DependTaskParameters {
                task_number,
                add_dependency: on,
                remove_dependency: remove,
            };

            match depend_task(&mut store, &storage, params) {
                Ok(task) => {
                    if let Some(dep_id) = on {
                        println!(
                            "✓ #{} \"{}\" now depends on #{}",
                            task.task_number,
                            task.title,
                            dep_id
                        );
                    } else {
                        println!(
                            "✓ Dependency removed from #{} \"{}\"",
                            task.task_number, task.title
                        );
                    }
                }
                Err(DependTaskError::TaskNotFound(id)) => {
                    eprintln!("Error: Task '{}' not found", id);
                    std::process::exit(1);
                }
                Err(DependTaskError::SelfDependency) => {
                    eprintln!("Error: A task cannot depend on itself");
                    std::process::exit(1);
                }
                Err(DependTaskError::CircularDependency) => {
                    eprintln!("Error: This would create a circular dependency");
                    std::process::exit(1);
                }
                Err(DependTaskError::AlreadyExists) => {
                    eprintln!("Error: Dependency already exists");
                    std::process::exit(1);
                }
                Err(DependTaskError::Storage(e)) => {
                    eprintln!("Error: Failed to save: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Edit {
            entity: EditEntity::Area { name, new_name },
        }) => {
            let params = EditAreaParameters { name, new_name };
            match edit_area(&mut store, &storage, params) {
                Ok(area) => {
                    println!("✓ Area updated: {}", area.name);
                }
                Err(EditAreaError::AreaNotFound(name)) => {
                    eprintln!("Error: Area '{}' not found", name);

                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for area in areas {
                            eprintln!("  - {}", area.name);
                        }
                    }
                    std::process::exit(1);
                }
                Err(EditAreaError::AmbiguousAreaName(query, names)) => {
                    eprintln!(
                        "Error: Area name '{}' is ambiguous. Multiple areas found:",
                        query
                    );
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(EditAreaError::AreaAlreadyExists(name)) => {
                    eprintln!("Error: Area with name '{}' already exists", name);
                    std::process::exit(1);
                }
                Err(EditAreaError::Storage(e)) => {
                    eprintln!("Error: Failed to update area: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Edit {
            entity:
                EditEntity::Project {
                    name,
                    new_name,
                    area,
                },
        }) => {
            let params = EditProjectParameters {
                name,
                new_name,
                area,
            };
            match edit_project(&mut store, &storage, params) {
                Ok(project) => {
                    println!("✓ Project updated: {}", project.name);
                }
                Err(EditProjectError::ProjectNotFound(name)) => {
                    eprintln!("Error: Project '{}' not found", name);

                    let projects: Vec<_> = store.get_active_projects().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for project in projects {
                            eprintln!("  - {}", project.name);
                        }
                    }
                    std::process::exit(1);
                }
                Err(EditProjectError::AmbiguousProjectName(query, names)) => {
                    eprintln!(
                        "Error: Project name '{}' is ambiguous. Multiple projects found:",
                        query
                    );
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(EditProjectError::ProjectAlreadyExists(name)) => {
                    eprintln!("Error: Project with name '{}' already exists", name);
                    std::process::exit(1);
                }
                Err(EditProjectError::AreaNotFound(area)) => {
                    eprintln!("Error: Area '{}' not found", area);

                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for a in areas {
                            eprintln!("  - {}", a.name);
                        }
                    }
                    std::process::exit(1);
                }
                Err(EditProjectError::AmbiguousAreaName(query, names)) => {
                    eprintln!(
                        "Error: Area name '{}' is ambiguous. Multiple areas found:",
                        query
                    );
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(EditProjectError::Storage(e)) => {
                    eprintln!("Error: Failed to update project: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Edit {
            entity: EditEntity::Task {
                task_number_or_fuzzy_name,
            },
        }) => {
            let params = EditTaskParameters {
                task_number_or_fuzzy_name,
            };

            match edit_task(&mut store, &storage, params) {
                Ok(task) => {
                    println!("✓ Task updated: {}", task.title);
                    println!("  #{}", task.task_number);
                }
                Err(EditTaskError::TaskNotFound(identifier)) => {
                    eprintln!("Error: Task '{}' not found", identifier);
                    std::process::exit(1);
                }
                Err(EditTaskError::AmbiguousTaskName(titles)) => {
                    eprintln!("Error: Task name is ambiguous. Multiple tasks found:");
                    for title in titles {
                        eprintln!("  - {}", title);
                    }
                    eprintln!("\nPlease be more specific or use the task number.");
                    std::process::exit(1);
                }
                Err(EditTaskError::EditorFailed(msg)) => {
                    eprintln!("Error: Failed to open editor: {}", msg);
                    eprintln!("\nMake sure $EDITOR or $VISUAL is set to a valid editor.");
                    std::process::exit(1);
                }
                Err(EditTaskError::ParseFailed(msg)) => {
                    eprintln!("Error: Failed to parse edited task: {}", msg);
                    std::process::exit(1);
                }
                Err(EditTaskError::NoChanges) => {
                    println!("No changes detected - task not modified");
                    std::process::exit(0);
                }
                Err(EditTaskError::ProjectNotFound(name)) => {
                    eprintln!("Error: Project '{}' not found", name);
                    eprintln!("\nUse 'tdo list projects' to see available projects.");
                    std::process::exit(1);
                }
                Err(EditTaskError::AmbiguousProjectName(names)) => {
                    eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(EditTaskError::AreaNotFound(name)) => {
                    eprintln!("Error: Area '{}' not found", name);
                    eprintln!("\nUse 'tdo list areas' to see available areas.");
                    std::process::exit(1);
                }
                Err(EditTaskError::AmbiguousAreaName(names)) => {
                    eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(EditTaskError::InvalidDate(field, error)) => {
                    eprintln!("Error: Invalid date for '{}': {}", field, error);
                    eprintln!("\nExpected format: YYYY-MM-DD (e.g., 2026-03-15)");
                    std::process::exit(1);
                }
                Err(EditTaskError::InvalidWhen(value)) => {
                    eprintln!("Error: Invalid 'when' value: {}", value);
                    eprintln!(
                        "\nExpected: inbox, today, today-evening, anytime, someday, or YYYY-MM-DD"
                    );
                    std::process::exit(1);
                }
                Err(EditTaskError::Storage(e)) => {
                    eprintln!("Error: Failed to save task: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Delete {
            task_numbers_or_fuzzy_names,
        }) => {
            for task_number_or_fuzzy_name in task_numbers_or_fuzzy_names {
                // Build parameters
                let params = DeleteTaskParameters {
                    task_number_or_fuzzy_name,
                };

                // Call service
                match delete_task(&mut store, &storage, params) {
                    Ok(task) => {
                        println!("✓ Task deleted: {}", task.title);
                        println!("  #{}", task.task_number);
                        println!("\nUse 'tdo trash' to view deleted items");
                        println!(
                            "Use 'tdo restore {}' to restore this task",
                            task.task_number
                        );
                    }
                    Err(DeleteTaskError::TaskNotFound(identifier)) => {
                        eprintln!("Error: Task '{}' not found", identifier);
                        std::process::exit(1);
                    }
                    Err(DeleteTaskError::AmbiguousTaskName(titles)) => {
                        eprintln!("Error: Task name is ambiguous. Multiple tasks found:");
                        for title in titles {
                            eprintln!("  - {}", title);
                        }
                        eprintln!("\nPlease be more specific or use the task number.");
                        std::process::exit(1);
                    }
                    Err(DeleteTaskError::TaskAlreadyDeleted(title)) => {
                        eprintln!("Error: Task '{}' is already deleted", title);
                        eprintln!("\nUse 'tdo trash' to view deleted items.");
                        std::process::exit(1);
                    }
                    Err(DeleteTaskError::Storage(e)) => {
                        eprintln!("Error: Failed to delete task: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Some(Commands::Restore { task_numbers }) => {
            for task_number_str in task_numbers {
                // Parse task number
                let parsed_task_number = match task_number_str.parse::<u64>() {
                    Ok(num) => num,
                    Err(_) => {
                        eprintln!("Error: Invalid task number '{}'", task_number_str);
                        eprintln!("\nTask number must be a numeric value.");
                        eprintln!("Use 'tdo trash' to see deleted tasks with their numbers.");
                        std::process::exit(1);
                    }
                };

                // Build parameters
                let params = RestoreTaskParameters {
                    task_number: parsed_task_number,
                };

                // Call service
                match restore_task(&mut store, &storage, params) {
                    Ok(task) => {
                        println!("✓ Task restored: {}", task.title);
                        println!("  #{}", task.task_number);
                        if let Some(ref project_key) = task.project_key
                            && let Some(project) = store.get_project(project_key)
                        {
                            println!("  Project: {}", project.name);
                        }
                    }
                    Err(RestoreTaskError::TaskNotFound(identifier)) => {
                        eprintln!("Error: Task '{}' not found", identifier);
                        eprintln!("\nUse 'tdo trash' to see deleted tasks.");
                        std::process::exit(1);
                    }
                    Err(RestoreTaskError::TaskNotDeleted(title)) => {
                        eprintln!("Error: Task '{}' is not deleted", title);
                        eprintln!(
                            "\nThis task is already active. Use 'tdo all' to see active tasks."
                        );
                        std::process::exit(1);
                    }
                    Err(RestoreTaskError::Storage(e)) => {
                        eprintln!("Error: Failed to restore task: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Some(Commands::Move {
            task_numbers,
            today,
            tomorrow,
            next_week,
            someday,
            on,
            due,
            clear_schedule,
            clear_deadline,
            project,
            area,
            clear_project,
            clear_area,
            tag,
            notes,
            defer_until,
            clear_defer,
            every,
            until,
            clear_recurrence,
        }) => {
            // Parse when flags (if any scheduling flag is provided)
            let when = if today || tomorrow || next_week || someday || on.is_some() {
                match When::from_command_flags(today, tomorrow, next_week, someday, on) {
                    Ok(w) => Some(w),
                    Err(WhenInstantiationError::ScheduleAtIncorrect(date_str, error)) => {
                        eprintln!("Error: Invalid schedule date '{}': {}", date_str, error);
                        std::process::exit(1);
                    }
                    Err(WhenInstantiationError::ConflictingFlags(flags)) => {
                        eprintln!("Error: Cannot use multiple scheduling flags together");
                        eprintln!("\nConflicting flags provided: {}", flags.join(", "));
                        eprintln!("\nPlease use only one of:");
                        eprintln!("  --today       Schedule for today");
                        eprintln!("  --tomorrow    Schedule for tomorrow");
                        eprintln!("  --next-week   Schedule for next Monday");
                        eprintln!("  --someday     Defer to someday");
                        eprintln!("  --on DATE     Schedule for a specific date");
                        std::process::exit(1);
                    }
                }
            } else {
                None
            };

            // Parse recurrence (shared across all task numbers in this move)
            let recurrence = if let Some(ref pattern) = every {
                let dtstart = match &when {
                    Some(When::Scheduled { date }) => *date,
                    _ => jiff::Zoned::now().date(),
                };
                let mut r = match parse_recurrence(pattern, dtstart) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Some(ref until_str) = until {
                    use saku_tdo::date_parser::parse_natural_date;
                    match parse_natural_date(until_str) {
                        Ok(d) => r.until = Some(d),
                        Err(e) => {
                            eprintln!("Error: Invalid --until date '{}': {}", until_str, e);
                            std::process::exit(1);
                        }
                    }
                }
                Some(r)
            } else {
                None
            };

            for task_number_str in task_numbers {
                let parsed_task_number = match task_number_str.parse::<u64>() {
                    Ok(num) => num,
                    Err(_) => {
                        eprintln!("Error: Invalid task number '{}'", task_number_str);
                        std::process::exit(1);
                    }
                };

                // Build parameters
                let params = MoveTaskParameters {
                    task_number: parsed_task_number,
                    notes: notes.clone(),
                    when: when.clone(),
                    deadline: due.clone(),
                    clear_schedule,
                    clear_deadline,
                    project: project.clone(),
                    area: area.clone(),
                    clear_project,
                    clear_area,
                    tags: tag.clone(),
                    defer_until: defer_until.clone(),
                    clear_defer,
                    recurrence: recurrence.clone(),
                    clear_recurrence,
                };

                // Call service
                match move_task(&mut store, &storage, params) {
                    Ok(task) => {
                        println!("✓ Task moved");
                        println!("  #{}", task.task_number);
                        if let Some(ref project_key) = task.project_key
                            && let Some(project) = store.get_project(project_key)
                        {
                            println!("  Project: {}", project.name);
                        }
                    }
                    Err(MoveTaskError::TaskNotFound(identifier)) => {
                        eprintln!("Error: Task '{}' not found", identifier);
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::AmbiguousTaskName(titles)) => {
                        eprintln!("Error: Task name is ambiguous. Multiple tasks found:");
                        for title in titles {
                            eprintln!("  - {}", title);
                        }
                        eprintln!("\nPlease be more specific or use the task number.");
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::AmbiguousTagName(names)) => {
                        eprintln!("Error: Tag name is ambiguous. Multiple tags found:");
                        for name in names {
                            eprintln!("  - {}", name);
                        }
                        eprintln!("\nPlease be more specific.");
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::TagNotFound(name)) => {
                        eprintln!("Error: Tag '{}' not found", name);
                        // TODO: Suggest existing tags if any
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::ProjectNotFound(name)) => {
                        eprintln!("Error: Project '{}' not found", name);

                        // Suggest existing projects if any
                        let projects: Vec<_> = store.projects.values().collect();
                        if !projects.is_empty() {
                            eprintln!("\nAvailable projects:");
                            for project in projects {
                                eprintln!("  - {}", project.name);
                            }
                        } else {
                            eprintln!(
                                "\nNo projects exist yet. Create one first or omit --project."
                            );
                        }
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::AmbiguousProjectName(names)) => {
                        eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                        for name in names {
                            eprintln!("  - {}", name);
                        }
                        eprintln!("\nPlease be more specific.");
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::AreaNotFound(name)) => {
                        eprintln!("Error: Area '{}' not found", name);

                        // Suggest existing areas if any
                        let areas: Vec<_> = store.areas.values().collect();
                        if !areas.is_empty() {
                            eprintln!("\nAvailable areas:");
                            for area in areas {
                                eprintln!("  - {}", area.name);
                            }
                        } else {
                            eprintln!("\nNo areas exist yet. Create one first or omit --area.");
                        }
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::AmbiguousAreaName(names)) => {
                        eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                        for name in names {
                            eprintln!("  - {}", name);
                        }
                        eprintln!("\nPlease be more specific.");
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::InvalidDeadline(date_str, error)) => {
                        eprintln!("Error: Invalid deadline '{}': {}", date_str, error);
                        std::process::exit(1);
                    }
                    Err(MoveTaskError::Storage(e)) => {
                        eprintln!("Error: Failed to save task: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Some(Commands::Create {
            entity: CreateEntity::Area { name },
        }) => {
            let params = CreateAreaParameters { name };
            match create_area(&mut store, &storage, params) {
                Ok(area) => {
                    println!("✓ Area {} created", area.name);
                }
                Err(CreateAreaError::AreaAlreadyExists(name)) => {
                    eprintln!("Error: Area with name '{}' already exists", name);
                    std::process::exit(1);
                }
                Err(CreateAreaError::Storage(e)) => {
                    eprintln!("Error: Failed to create area: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Remove {
            entity: RemoveEntity::Area { name },
        }) => {
            let params = DeleteAreaParameters { name };

            match delete_area(&mut store, &storage, params) {
                Ok(result) => {
                    println!("✓ Area deleted: {}", result.area.name);
                    if result.cascaded_projects_count > 0 {
                        println!(
                            "  └─ {} project(s) also deleted",
                            result.cascaded_projects_count
                        );
                    }
                    if result.cascaded_tasks_count > 0 {
                        println!("  └─ {} task(s) also deleted", result.cascaded_tasks_count);
                    }
                }
                Err(DeleteAreaError::AreaNotFound(name)) => {
                    eprintln!("Error: Area '{}' not found", name);

                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for area in areas {
                            eprintln!("  - {}", area.name);
                        }
                    }
                    std::process::exit(1);
                }
                Err(DeleteAreaError::Storage(e)) => {
                    eprintln!("Error: Failed to delete area: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::List {
            entity: ListEntity::Areas,
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            let mut areas: Vec<_> = store.get_active_areas().collect();
            areas.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            if matches!(fmt, output::OutputFormat::Pretty) {
                if areas.is_empty() {
                    println!("No areas found");
                } else {
                    println!(
                        "{} ({} {})\n",
                        "AREAS".cyan(),
                        areas.len(),
                        if areas.len() == 1 { "area" } else { "areas" }
                    );
                    for area in areas {
                        let area_key = area.storage_key();
                        let project_count = store
                            .get_projects_for_area(&area_key)
                            .filter(|p| p.deleted_at.is_none())
                            .count();
                        let direct_task_count = store
                            .get_tasks_for_area(&area_key)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let project_task_count: usize = store
                            .get_projects_for_area(&area_key)
                            .filter(|p| p.deleted_at.is_none())
                            .map(|p| {
                                store
                                    .get_tasks_for_project(&p.storage_key())
                                    .filter(|t| t.deleted_at.is_none())
                                    .count()
                            })
                            .sum();
                        let total_task_count = direct_task_count + project_task_count;
                        println!("{} {}", "•".green(), area.name.bold());
                        println!(
                            "    {} {} {} {}",
                            project_count.to_string().dimmed(),
                            if project_count == 1 { "project" } else { "projects" },
                            "•".dimmed(),
                            format!(
                                "{} {}",
                                total_task_count,
                                if total_task_count == 1 { "task" } else { "tasks" }
                            )
                            .dimmed()
                        );
                        println!("    {}", "─".repeat(30).dimmed());
                        println!();
                    }
                }
            } else {
                let out: Vec<_> = areas
                    .iter()
                    .map(|area| {
                        let area_key = area.storage_key();
                        let project_count = store
                            .get_projects_for_area(&area_key)
                            .filter(|p| p.deleted_at.is_none())
                            .count();
                        let direct_task_count = store
                            .get_tasks_for_area(&area_key)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let project_task_count: usize = store
                            .get_projects_for_area(&area_key)
                            .filter(|p| p.deleted_at.is_none())
                            .map(|p| {
                                store
                                    .get_tasks_for_project(&p.storage_key())
                                    .filter(|t| t.deleted_at.is_none())
                                    .count()
                            })
                            .sum();
                        output::AreaOutput {
                            name: area.name.clone(),
                            project_count,
                            task_count: direct_task_count + project_task_count,
                        }
                    })
                    .collect();
                match fmt {
                    output::OutputFormat::Json => output::print_json(&out),
                    output::OutputFormat::Csv => output::print_csv(&out),
                    output::OutputFormat::Pretty => unreachable!(),
                }
            }
        }
        Some(Commands::Create {
            entity: CreateEntity::Project { name, area },
        }) => {
            let params = CreateProjectParameters { name, area };
            match create_project(&mut store, &storage, params) {
                Ok(project) => {
                    println!("✓ Project {} created", project.name);
                }
                Err(CreateProjectError::AreaNotFound(area)) => {
                    eprintln!("Error: Area with name '{}' not found", area);
                    std::process::exit(1);
                }
                Err(CreateProjectError::AmbiguousAreaName(names)) => {
                    eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(CreateProjectError::ProjectAlreadyExists(name)) => {
                    eprintln!("Error: Project with name '{}' already exists", name);
                    std::process::exit(1);
                }
                Err(CreateProjectError::Storage(e)) => {
                    eprintln!("Error: Failed to create project: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Remove {
            entity: RemoveEntity::Project { name },
        }) => {
            let params = DeleteProjectParameters { name };

            match delete_project(&mut store, &storage, params) {
                Ok(result) => {
                    println!("✓ Project deleted: {}", result.project.name);
                    if result.cascaded_tasks_count > 0 {
                        println!("  └─ {} task(s) also deleted", result.cascaded_tasks_count);
                    }
                }
                Err(DeleteProjectError::ProjectNotFound(name)) => {
                    eprintln!("Error: Project '{}' not found", name);

                    let projects: Vec<_> = store.get_active_projects().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for project in projects {
                            eprintln!("  - {}", project.name);
                        }
                    }
                    std::process::exit(1);
                }
                Err(DeleteProjectError::AmbiguousProjectName(names)) => {
                    eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                    for name in names {
                        eprintln!("  - {}", name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
                Err(DeleteProjectError::ProjectAlreadyDeleted(name)) => {
                    eprintln!("Error: Project '{}' is already deleted", name);
                    std::process::exit(1);
                }
                Err(DeleteProjectError::Storage(e)) => {
                    eprintln!("Error: Failed to delete project: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::List {
            entity: ListEntity::Projects,
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            let mut projects: Vec<_> = store.get_active_projects().collect();
            projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            if matches!(fmt, output::OutputFormat::Pretty) {
                if projects.is_empty() {
                    println!("No projects found");
                } else {
                    println!(
                        "{} ({} {})\n",
                        "PROJECTS".cyan(),
                        projects.len(),
                        if projects.len() == 1 { "project" } else { "projects" }
                    );
                    for project in projects {
                        let task_count = store
                            .get_tasks_for_project(&project.storage_key())
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        println!("{} {}", "•".green(), project.name.bold());
                        if let Some(ref area_key) = project.area_key
                            && let Some(area) = store.get_area(area_key)
                        {
                            println!("    {} {}", "Area:".dimmed(), area.name.blue());
                        }
                        println!(
                            "    {} {}",
                            task_count.to_string().dimmed(),
                            if task_count == 1 { "task" } else { "tasks" }.dimmed()
                        );
                        println!("    {}", "─".repeat(30).dimmed());
                        println!();
                    }
                }
            } else {
                let out: Vec<_> = projects
                    .iter()
                    .map(|project| {
                        let task_count = store
                            .get_tasks_for_project(&project.storage_key())
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let area = project
                            .area_key
                            .as_ref()
                            .and_then(|key| store.get_area(key))
                            .map(|a| a.name.clone());
                        output::ProjectOutput {
                            name: project.name.clone(),
                            area,
                            task_count,
                        }
                    })
                    .collect();
                match fmt {
                    output::OutputFormat::Json => output::print_json(&out),
                    output::OutputFormat::Csv => output::print_csv(&out),
                    output::OutputFormat::Pretty => unreachable!(),
                }
            }
        }
        Some(Commands::Show {
            entity: ShowEntity::Project { name },
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            let matching: Vec<_> = store
                .get_active_projects()
                .filter(|p| p.name.to_lowercase().contains(&name.to_lowercase()))
                .collect();

            let project = match matching.len() {
                0 => {
                    eprintln!("Error: Project '{}' not found", name);
                    let projects: Vec<_> = store.get_active_projects().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for p in projects {
                            eprintln!("  - {}", p.name);
                        }
                    }
                    std::process::exit(1);
                }
                1 => matching[0],
                _ => {
                    eprintln!("Error: Project name is ambiguous. Multiple projects found:");
                    for p in &matching {
                        eprintln!("  - {}", p.name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
            };

            let mut tasks: Vec<_> = store
                .get_tasks_for_project(&project.storage_key())
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                .collect();
            tasks.sort_by_key(|t| t.task_number);

            match fmt {
                output::OutputFormat::Json => {
                    let out: Vec<_> = tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    output::print_json(&out);
                }
                output::OutputFormat::Csv => {
                    let out: Vec<_> = tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    output::print_csv(&out);
                }
                output::OutputFormat::Pretty => {
                    let header = if let Some(ref area_key) = project.area_key {
                        if let Some(area) = store.get_area(area_key) {
                            format!("{} ({})", project.name, area.name)
                        } else {
                            project.name.clone()
                        }
                    } else {
                        project.name.clone()
                    };
                    if tasks.is_empty() {
                        println!("No tasks in project '{}'", header);
                    } else {
                        saku_tdo::ui::render_view_header(&header, tasks.len());
                        for task in tasks {
                            saku_tdo::ui::render_task_line(task, &store);
                            saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                        }
                    }
                }
            }
        }
        Some(Commands::Show {
            entity: ShowEntity::Area { name },
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            let matching: Vec<_> = store
                .get_active_areas()
                .filter(|a| a.name.to_lowercase().contains(&name.to_lowercase()))
                .collect();

            let area = match matching.len() {
                0 => {
                    eprintln!("Error: Area '{}' not found", name);
                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for a in areas {
                            eprintln!("  - {}", a.name);
                        }
                    }
                    std::process::exit(1);
                }
                1 => matching[0],
                _ => {
                    eprintln!("Error: Area name is ambiguous. Multiple areas found:");
                    for a in &matching {
                        eprintln!("  - {}", a.name);
                    }
                    eprintln!("\nPlease be more specific.");
                    std::process::exit(1);
                }
            };

            let area_key = area.storage_key();
            let mut direct_tasks: Vec<_> = store
                .get_tasks_for_area(&area_key)
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                .collect();
            direct_tasks.sort_by_key(|t| t.task_number);

            let mut projects: Vec<_> = store
                .get_projects_for_area(&area_key)
                .filter(|p| p.deleted_at.is_none())
                .collect();
            projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            let project_tasks: Vec<_> = projects
                .iter()
                .map(|p| {
                    let mut tasks: Vec<_> = store
                        .get_tasks_for_project(&p.storage_key())
                        .filter(|t| t.parent_task_key.is_none())
                        .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                        .collect();
                    tasks.sort_by_key(|t| t.task_number);
                    (*p, tasks)
                })
                .filter(|(_, tasks)| !tasks.is_empty())
                .collect();

            let total_tasks = direct_tasks.len()
                + project_tasks
                    .iter()
                    .map(|(_, tasks)| tasks.len())
                    .sum::<usize>();

            match fmt {
                output::OutputFormat::Json | output::OutputFormat::Csv => {
                    let all_tasks: Vec<&saku_tdo::models::task::Task> = direct_tasks
                        .iter()
                        .chain(project_tasks.iter().flat_map(|(_, tasks)| tasks.iter()))
                        .copied()
                        .collect();
                    let out: Vec<_> = all_tasks
                        .iter()
                        .map(|t| output::TaskOutput::from_task(t, &store))
                        .collect();
                    match fmt {
                        output::OutputFormat::Json => output::print_json(&out),
                        output::OutputFormat::Csv => output::print_csv(&out),
                        output::OutputFormat::Pretty => unreachable!(),
                    }
                }
                output::OutputFormat::Pretty => {
                    if total_tasks == 0 {
                        println!("No tasks in area '{}'", area.name);
                    } else {
                        saku_tdo::ui::render_view_header(&area.name, total_tasks);
                        for task in &direct_tasks {
                            saku_tdo::ui::render_task_line(task, &store);
                            saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                        }
                        for (project, tasks) in project_tasks.iter() {
                            saku_tdo::ui::render_section_header(&project.name);
                            for task in tasks {
                                saku_tdo::ui::render_task_line(task, &store);
                                saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                            }
                        }
                    }
                }
            }
        }
        Some(Commands::List {
            entity: ListEntity::Tags,
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            use std::collections::HashMap;
            let mut tag_counts: HashMap<String, usize> = HashMap::new();
            for task in store.get_active_tasks().filter(|t| t.completed_at.is_none()) {
                for tag in &task.tags {
                    *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
            let mut tags: Vec<_> = tag_counts.iter().collect();
            tags.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

            if matches!(fmt, output::OutputFormat::Pretty) {
                if tags.is_empty() {
                    println!("No tags found");
                } else {
                    println!(
                        "{} ({} {})\n",
                        "TAGS".cyan(),
                        tags.len(),
                        if tags.len() == 1 { "tag" } else { "tags" }
                    );
                    for (tag, count) in tags {
                        println!(
                            "  {} {} {}",
                            "•".green(),
                            tag.bold(),
                            format!("({} {})", count, if *count == 1 { "task" } else { "tasks" })
                                .dimmed()
                        );
                    }
                }
            } else {
                let out: Vec<_> = tags
                    .iter()
                    .map(|(name, count)| output::TagOutput {
                        name: name.to_string(),
                        task_count: **count,
                    })
                    .collect();
                match fmt {
                    output::OutputFormat::Json => output::print_json(&out),
                    output::OutputFormat::Csv => output::print_csv(&out),
                    output::OutputFormat::Pretty => unreachable!(),
                }
            }
        }
        Some(Commands::Show {
            entity: ShowEntity::Tag { name },
            json,
            csv,
        }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            let mut tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| t.parent_task_key.is_none())
                .filter(|t| {
                    t.completed_at.is_none()
                        && t.tags
                            .iter()
                            .any(|tag| tag.to_lowercase() == name.to_lowercase())
                })
                .collect();
            tasks.sort_by_key(|t| t.task_number);

            if matches!(fmt, output::OutputFormat::Pretty) {
                if tasks.is_empty() {
                    println!("No tasks with tag '{}'", name);
                    use std::collections::HashSet;
                    let available_tags: HashSet<_> = store
                        .get_active_tasks()
                        .filter(|t| t.completed_at.is_none())
                        .flat_map(|t| &t.tags)
                        .collect();
                    if !available_tags.is_empty() {
                        println!("\nAvailable tags:");
                        for tag in available_tags {
                            println!("  - {}", tag);
                        }
                    }
                } else {
                    saku_tdo::ui::render_view_header(&format!("#{}", name), tasks.len());
                    for task in tasks {
                        saku_tdo::ui::render_task_line(task, &store);
                        saku_tdo::ui::render_subtask_children(&task.storage_key(), &store);
                    }
                }
            } else {
                let out: Vec<_> = tasks
                    .iter()
                    .map(|t| output::TaskOutput::from_task(t, &store))
                    .collect();
                match fmt {
                    output::OutputFormat::Json => output::print_json(&out),
                    output::OutputFormat::Csv => output::print_csv(&out),
                    output::OutputFormat::Pretty => unreachable!(),
                }
            }
        }
        #[cfg(feature = "sync")]
        Some(Commands::Sync { action }) => {
            match action {
                SyncAction::Login { server, email } => {
                    // Prompt for password
                    let password = match rpassword::prompt_password("Password: ") {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("Error reading password: {}", e);
                            std::process::exit(1);
                        }
                    };

                    // Prompt for encryption passphrase
                    let passphrase = match rpassword::prompt_password("Encryption passphrase: ") {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("Error reading passphrase: {}", e);
                            std::process::exit(1);
                        }
                    };
                    if passphrase.is_empty() {
                        eprintln!("Error: Encryption passphrase cannot be empty");
                        std::process::exit(1);
                    }

                    // Get device ID
                    let device_id = match saku_storage::device::get_or_create_device_id() {
                        Ok(id) => id,
                        Err(e) => {
                            eprintln!("Error getting device ID: {}", e);
                            std::process::exit(1);
                        }
                    };

                    // Login to server
                    let agent = ureq::agent();
                    let login_url = format!("{}/api/v1/auth/login", server.trim_end_matches('/'));
                    let resp = match agent.post(&login_url).send_json(ureq::json!({
                        "email": email,
                        "password": password,
                        "device_id": device_id,
                        "device_name": hostname(),
                    })) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("Error: Login failed: {}", e);
                            std::process::exit(1);
                        }
                    };

                    let body: serde_json::Value = match resp.into_json() {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Error: Failed to parse response: {}", e);
                            std::process::exit(1);
                        }
                    };

                    let access_token = body["access_token"].as_str().unwrap_or("");
                    let refresh_token = body["refresh_token"].as_str().unwrap_or("");

                    // Store all credentials in a single keychain entry
                    if let Err(e) =
                        saku_crypto::keychain::SyncCredentialStore::new().and_then(|s| {
                            s.store(&saku_crypto::keychain::SyncCredentials {
                                access_token: Some(access_token.to_string()),
                                refresh_token: Some(refresh_token.to_string()),
                                passphrase: Some(passphrase.clone()),
                            })
                        })
                    {
                        eprintln!("Warning: Failed to store credentials in keychain: {}", e);
                    }

                    // Save config
                    let sync_config = saku_sync::config::SyncClientConfig {
                        server_url: server.clone(),
                        device_id,
                    };
                    if let Err(e) = saku_sync::config::save_sync_config(&sync_config) {
                        eprintln!("Error: Failed to save sync config: {}", e);
                        std::process::exit(1);
                    }

                    println!("Logged in to {} as {}", server, email);
                }
                SyncAction::Logout => {
                    // Clear consolidated keychain entry
                    if let Ok(s) = saku_crypto::keychain::SyncCredentialStore::new() {
                        let _ = s.delete();
                    }
                    // Also clean up legacy entries for users who haven't migrated
                    for account in &[
                        "saku-sync-access-token",
                        "saku-sync-refresh-token",
                        "saku-sync-passphrase",
                    ] {
                        if let Ok(ks) = saku_crypto::keychain::KeychainStore::new(account) {
                            let _ = ks.delete();
                        }
                    }
                    // Delete config
                    if let Err(e) = saku_sync::config::delete_sync_config() {
                        eprintln!("Warning: Failed to delete sync config: {}", e);
                    }
                    println!("Logged out and cleared sync credentials");
                }
                SyncAction::Status => {
                    match saku_sync::config::load_sync_config() {
                        Ok(Some(config)) => {
                            println!("Sync configured:");
                            println!("  Server: {}", config.server_url);
                            println!("  Device: {}", config.device_id);

                            // Check if passphrase is stored
                            let has_passphrase =
                                saku_crypto::keychain::SyncCredentialStore::new()
                                    .and_then(|s| s.load_or_migrate())
                                    .map(|c| c.passphrase.is_some())
                                    .unwrap_or(false);
                            println!(
                                "  Passphrase: {}",
                                if has_passphrase { "stored" } else { "not set" }
                            );
                        }
                        Ok(None) => {
                            println!("Sync not configured. Use 'tdo sync login' to set up.");
                        }
                        Err(e) => {
                            eprintln!("Error reading sync config: {}", e);
                        }
                    }
                }
            }
            return;
        }
        Some(Commands::Completion { shell }) => {
            let mut cmd = Cli::command();
            let bin_name = "tdo";
            clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            return;
        }
        None => {
            // Default: show today view (same as `tdo today`)
            let no_filters = ViewFilters {
                project: None,
                tags: vec![],
                area: None,
                ready: false,
            };
            render_today_view(&store, false, &no_filters);
        }
    }

    // After mutation, attempt sync if TDO_SYNC_DIR is set
    if should_sync {
        try_sync(&storage_path);
    }
}
