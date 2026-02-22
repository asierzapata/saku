use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use colored::*;

use saku_tdo::{
    models::task::{When, WhenInstantiationError},
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
        #[arg(long, conflicts_with = "csv")]
        json: bool,

        /// Output as CSV
        #[arg(long, conflicts_with = "json")]
        csv: bool,

        #[command(subcommand)]
        entity: ViewEntity,
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

        /// Add tags (can be used multiple times)
        #[arg(short, long, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Add notes
        #[arg(short, long)]
        notes: Option<String>,
    },

    /// Complete one or more tasks
    Done {
        /// Task number(s) or fuzzy name(s) - provide multiple to complete several tasks at once
        #[arg(num_args(1..))]
        task_numbers_or_fuzzy_names: Vec<String>,
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

#[derive(Debug, Subcommand)]
enum ViewEntity {
    /// Show today's tasks (including overdue)
    Today,
    /// List tasks in the inbox
    Inbox,
    /// Show upcoming tasks (future-dated)
    Upcoming,
    /// Show someday tasks
    Someday,
    /// Show all tasks with deadlines, grouped by urgency
    Deadlines,
    /// Show completed tasks (last 14 days)
    Logbook,
    /// Show deleted items
    Trash,
    /// Show all active tasks
    All,
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
fn render_today_view(store: &saku_tdo::models::store::Store) {
    let today = jiff::Zoned::now().date();

    // Collect overdue tasks (scheduled or deadline < today)
    let overdue_tasks: Vec<_> = store
        .get_active_tasks()
        .filter(|t| {
            t.completed_at.is_none() && {
                let scheduled_overdue = match t.when {
                    When::Scheduled { date, .. } => date < today,
                    _ => false,
                };
                let deadline_overdue = t.deadline.is_some_and(|d| d < today);
                scheduled_overdue || deadline_overdue
            }
        })
        .collect();
    let overdue_tasks = saku_tdo::models::task::order_tasks_with_store(overdue_tasks, store);

    // Collect today tasks (scheduled or deadline == today, excluding overdue)
    let today_current: Vec<_> = store
        .get_active_tasks()
        .filter(|t| {
            t.completed_at.is_none() && {
                let scheduled_today = match t.when {
                    When::Scheduled { date, .. } => date == today,
                    _ => false,
                };
                let deadline_today = t.deadline == Some(today);

                // Check if not already in overdue
                let is_overdue = match t.when {
                    When::Scheduled { date, .. } if date < today => true,
                    _ => t.deadline.is_some_and(|d| d < today),
                };

                (scheduled_today || deadline_today) && !is_overdue
            }
        })
        .collect();
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
            }
        }

        // Show today tasks
        if !today_current.is_empty() {
            for task in today_current {
                saku_tdo::ui::render_task_line(task, store);
            }
        }
    }
}

/// Attempt to sync after a mutation.
/// Tries server sync first (if configured), falls back to TDO_SYNC_DIR for local dev.
/// Sync is best-effort: errors are printed as warnings but never abort.
fn try_sync(storage_path: &std::path::Path) {
    // Try server sync first (if the sync feature is enabled and configured)
    #[cfg(feature = "sync")]
    {
        if let Ok(Some(config)) = saku_sync::config::load_sync_config() {
            // Read passphrase from keychain
            match saku_crypto::keychain::KeychainStore::new("saku-sync-passphrase")
                .and_then(|ks| ks.get_passphrase())
            {
                Ok(passphrase) => {
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
                Err(_) => {
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
            render_today_view(&store);
        }
        Some(Commands::Inbox) => {
            eprintln!(
                "{}",
                "Warning: 'tdo inbox' is deprecated. Use 'tdo view inbox' instead.".yellow()
            );
            eprintln!("{}", "This command will be removed in v1.0.0.".yellow());
            eprintln!();

            // Filter inbox tasks
            let inbox_tasks: Vec<_> = store
                .get_active_tasks()
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

            // Filter someday tasks
            let someday_tasks: Vec<_> = store
                .get_active_tasks()
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

            // Collect all active, incomplete tasks
            let all_tasks: Vec<_> = store.get_active_tasks().collect();
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

            // Collect upcoming tasks (scheduled or deadline in the future)
            let upcoming_tasks: Vec<_> = store
                .get_active_tasks()
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

            // Collect completed tasks from last 14 days
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

            // Collect deleted items
            let deleted_tasks: Vec<_> = store.get_deleted_tasks().collect();
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
        Some(Commands::View { entity, json, csv }) => {
            let fmt = output::OutputFormat::from_flags(json, csv);
            match entity {
                ViewEntity::Today => {
                    if matches!(fmt, output::OutputFormat::Pretty) {
                        render_today_view(&store);
                    } else {
                        let today = jiff::Zoned::now().date();
                        let tasks: Vec<_> = store
                            .get_active_tasks()
                            .filter(|t| {
                                t.completed_at.is_none() && {
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
                ViewEntity::Inbox => {
                    let inbox_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| matches!(t.when, When::Inbox))
                        .filter(|t| t.completed_at.is_none())
                        .collect();
                    if matches!(fmt, output::OutputFormat::Pretty) {
                        let inbox_tasks =
                            saku_tdo::models::task::order_tasks_with_store(inbox_tasks, &store);
                        if inbox_tasks.is_empty() {
                            println!("Inbox is empty");
                        } else {
                            saku_tdo::ui::render_view_header("Inbox", inbox_tasks.len());
                            for task in inbox_tasks {
                                saku_tdo::ui::render_task_line(task, &store);
                            }
                        }
                    } else {
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
                }
                ViewEntity::Someday => {
                    let someday_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| matches!(t.when, When::Someday))
                        .filter(|t| t.completed_at.is_none())
                        .collect();
                    if matches!(fmt, output::OutputFormat::Pretty) {
                        let someday_tasks =
                            saku_tdo::models::task::order_tasks_with_store(someday_tasks, &store);
                        if someday_tasks.is_empty() {
                            println!("No someday tasks");
                        } else {
                            saku_tdo::ui::render_view_header("Someday", someday_tasks.len());
                            for task in someday_tasks {
                                saku_tdo::ui::render_task_line(task, &store);
                            }
                        }
                    } else {
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
                }
                ViewEntity::All => {
                    use std::collections::HashMap;

                    let all_tasks: Vec<_> = store.get_active_tasks().collect();
                    if matches!(fmt, output::OutputFormat::Pretty) {
                        let all_tasks =
                            saku_tdo::models::task::order_tasks_with_store(all_tasks, &store);
                        if all_tasks.is_empty() {
                            println!("No active tasks");
                        } else {
                            let mut grouped: HashMap<
                                String,
                                Vec<&saku_tdo::models::task::Task>,
                            > = HashMap::new();
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
                                        saku_tdo::ui::render_task_line(task, &store);
                                    }
                                }
                            }
                        }
                    } else {
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
                }
                ViewEntity::Upcoming => {
                    use jiff::civil::Date;
                    use std::collections::BTreeMap;

                    let today = jiff::Zoned::now().date();

                    let upcoming_tasks: Vec<_> = store
                        .get_active_tasks()
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

                    if matches!(fmt, output::OutputFormat::Pretty) {
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
                                saku_tdo::ui::render_section_header(
                                    &saku_tdo::ui::format_date_header(date),
                                );
                                for task in tasks {
                                    saku_tdo::ui::render_task_line(task, &store);
                                }
                            }
                        }
                    } else {
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
                }
                ViewEntity::Deadlines => {
                    let today = jiff::Zoned::now().date();

                    let mut deadline_tasks: Vec<_> = store
                        .get_active_tasks()
                        .filter(|t| t.completed_at.is_none() && t.deadline.is_some())
                        .collect();

                    if matches!(fmt, output::OutputFormat::Pretty) {
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
                                saku_tdo::ui::render_section_header(&format!(
                                    "Overdue ({})",
                                    overdue.len()
                                ));
                                for task in overdue {
                                    saku_tdo::ui::render_task_line(task, &store);
                                }
                            }
                            if !due_today.is_empty() {
                                saku_tdo::ui::render_section_header(&format!(
                                    "Today ({})",
                                    due_today.len()
                                ));
                                for task in due_today {
                                    saku_tdo::ui::render_task_line(task, &store);
                                }
                            }
                            if !this_week.is_empty() {
                                saku_tdo::ui::render_section_header(&format!(
                                    "This Week ({})",
                                    this_week.len()
                                ));
                                for task in this_week {
                                    saku_tdo::ui::render_task_line(task, &store);
                                }
                            }
                            if !later.is_empty() {
                                saku_tdo::ui::render_section_header(&format!(
                                    "Later ({})",
                                    later.len()
                                ));
                                for task in later {
                                    saku_tdo::ui::render_task_line(task, &store);
                                }
                            }
                        }
                    } else {
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

                    if matches!(fmt, output::OutputFormat::Pretty) {
                        if completed_tasks.is_empty() {
                            println!("No completed tasks in the last 14 days");
                        } else {
                            let mut grouped: BTreeMap<
                                (i16, i8),
                                Vec<&saku_tdo::models::task::Task>,
                            > = BTreeMap::new();
                            for task in &completed_tasks {
                                if let Some(completed_at) = task.completed_at {
                                    let year_month = saku_tdo::ui::get_year_month(completed_at);
                                    grouped.entry(year_month).or_default().push(task);
                                }
                            }
                            saku_tdo::ui::render_view_header("Logbook", completed_tasks.len());
                            for (_year_month, tasks) in grouped.iter().rev() {
                                let mut sorted_tasks = tasks.clone();
                                sorted_tasks.sort_by(|a, b| {
                                    b.completed_at.unwrap().cmp(&a.completed_at.unwrap())
                                });
                                let month_header = saku_tdo::ui::format_month_header(
                                    sorted_tasks[0].completed_at.unwrap(),
                                );
                                saku_tdo::ui::render_section_header(&month_header);
                                for task in sorted_tasks {
                                    saku_tdo::ui::render_task_line_with_completion_date(
                                        task, &store,
                                    );
                                }
                            }
                        }
                    } else {
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
                }
                ViewEntity::Trash => {
                    let deleted_tasks: Vec<_> = store.get_deleted_tasks().collect();
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
                        .get_tasks_for_project(project.id)
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
                            let header = if let Some(area_id) = project.area_id {
                                if let Some(area) = store.get_area(area_id) {
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

                    let mut direct_tasks: Vec<_> = store
                        .get_tasks_for_area(area.id)
                        .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                        .collect();
                    direct_tasks.sort_by_key(|t| t.task_number);

                    let mut projects: Vec<_> = store
                        .get_projects_for_area(area.id)
                        .filter(|p| p.deleted_at.is_none())
                        .collect();
                    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    let project_tasks: Vec<_> = projects
                        .iter()
                        .map(|p| {
                            let mut tasks: Vec<_> = store
                                .get_tasks_for_project(p.id)
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
                                .chain(
                                    project_tasks
                                        .iter()
                                        .flat_map(|(_, tasks)| tasks.iter()),
                                )
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
                                }
                                for (i, (project, tasks)) in project_tasks.iter().enumerate() {
                                    if !direct_tasks.is_empty() || i > 0 {
                                        saku_tdo::ui::render_section_header(&project.name);
                                    } else {
                                        println!("  ─── {} ───\n", project.name.bold());
                                    }
                                    for task in tasks {
                                        saku_tdo::ui::render_task_line(task, &store);
                                    }
                                }
                            }
                        }
                    }
                }
                ViewEntity::Tag { name } => {
                    let mut tasks: Vec<_> = store
                        .get_active_tasks()
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
            }
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

            // Build parameters
            let params = AddTaskParameters {
                title,
                notes,
                when,
                deadline: due,
                project,
                area,
                tags: tag,
            };

            // Call service
            match add_task(&mut store, &storage, params) {
                Ok(task) => {
                    println!("✓ Task added: {}", task.title);
                    println!("  #{}", task.task_number);
                    if let Some(project_id) = task.project_id
                        && let Some(project) = store.get_project(project_id)
                    {
                        println!("  Project: {}", project.name);
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
                Err(AddTaskError::Storage(e)) => {
                    eprintln!("Error: Failed to save task: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Done {
            task_numbers_or_fuzzy_names,
        }) => {
            for task_number_or_fuzzy_name in task_numbers_or_fuzzy_names {
                // Build parameters
                let params = CompleteTaskParameters {
                    task_number_or_fuzzy_name,
                };

                // Call service
                match complete_task(&mut store, &storage, params) {
                    Ok(result) => {
                        println!("✓ Task completed: {}", result.task.title);
                        println!("  #{}", result.task.task_number);
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
                        if let Some(project_id) = task.project_id
                            && let Some(project) = store.get_project(project_id)
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
            tag,
            notes,
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
                    tags: tag.clone(),
                };

                // Call service
                match move_task(&mut store, &storage, params) {
                    Ok(task) => {
                        println!("✓ Task moved");
                        println!("  #{}", task.task_number);
                        if let Some(project_id) = task.project_id
                            && let Some(project) = store.get_project(project_id)
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
                        let project_count = store
                            .get_projects_for_area(area.id)
                            .filter(|p| p.deleted_at.is_none())
                            .count();
                        let direct_task_count = store
                            .get_tasks_for_area(area.id)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let project_task_count: usize = store
                            .get_projects_for_area(area.id)
                            .filter(|p| p.deleted_at.is_none())
                            .map(|p| {
                                store
                                    .get_tasks_for_project(p.id)
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
                        let project_count = store
                            .get_projects_for_area(area.id)
                            .filter(|p| p.deleted_at.is_none())
                            .count();
                        let direct_task_count = store
                            .get_tasks_for_area(area.id)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let project_task_count: usize = store
                            .get_projects_for_area(area.id)
                            .filter(|p| p.deleted_at.is_none())
                            .map(|p| {
                                store
                                    .get_tasks_for_project(p.id)
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
                            .get_tasks_for_project(project.id)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        println!("{} {}", "•".green(), project.name.bold());
                        if let Some(area_id) = project.area_id
                            && let Some(area) = store.get_area(area_id)
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
                            .get_tasks_for_project(project.id)
                            .filter(|t| t.deleted_at.is_none())
                            .count();
                        let area = project
                            .area_id
                            .and_then(|id| store.get_area(id))
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
                .get_tasks_for_project(project.id)
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
                    let header = if let Some(area_id) = project.area_id {
                        if let Some(area) = store.get_area(area_id) {
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

            let mut direct_tasks: Vec<_> = store
                .get_tasks_for_area(area.id)
                .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                .collect();
            direct_tasks.sort_by_key(|t| t.task_number);

            let mut projects: Vec<_> = store
                .get_projects_for_area(area.id)
                .filter(|p| p.deleted_at.is_none())
                .collect();
            projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            let project_tasks: Vec<_> = projects
                .iter()
                .map(|p| {
                    let mut tasks: Vec<_> = store
                        .get_tasks_for_project(p.id)
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
                        }
                        for (i, (project, tasks)) in project_tasks.iter().enumerate() {
                            if !direct_tasks.is_empty() || i > 0 {
                                saku_tdo::ui::render_section_header(&project.name);
                            } else {
                                println!("  ─── {} ───\n", project.name.bold());
                            }
                            for task in tasks {
                                saku_tdo::ui::render_task_line(task, &store);
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

                    // Store tokens in keychain
                    if let Err(e) =
                        saku_crypto::keychain::KeychainStore::new("saku-sync-access-token")
                            .and_then(|ks| ks.store_passphrase(access_token))
                    {
                        eprintln!("Warning: Failed to store access token in keychain: {}", e);
                    }
                    if let Err(e) =
                        saku_crypto::keychain::KeychainStore::new("saku-sync-refresh-token")
                            .and_then(|ks| ks.store_passphrase(refresh_token))
                    {
                        eprintln!("Warning: Failed to store refresh token in keychain: {}", e);
                    }
                    if let Err(e) =
                        saku_crypto::keychain::KeychainStore::new("saku-sync-passphrase")
                            .and_then(|ks| ks.store_passphrase(&passphrase))
                    {
                        eprintln!("Warning: Failed to store passphrase in keychain: {}", e);
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
                    // Clear keychain entries
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
                                saku_crypto::keychain::KeychainStore::new("saku-sync-passphrase")
                                    .and_then(|ks| ks.get_passphrase())
                                    .is_ok();
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
            render_today_view(&store);
        }
    }

    // After mutation, attempt sync if TDO_SYNC_DIR is set
    if should_sync {
        try_sync(&storage_path);
    }
}
