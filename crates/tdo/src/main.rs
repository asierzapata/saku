use std::path::PathBuf;

use clap::{Parser, Subcommand};
use colored::*;

use crate::{
    models::task::{When, WhenInstantiationError},
    services::{
        areas::{
            CreateAreaError, CreateAreaParameters, DeleteAreaError, DeleteAreaParameters,
            create_area, delete_area,
        },
        projects::{
            CreateProjectError, CreateProjectParameters, DeleteProjectError,
            DeleteProjectParameters, create_project, delete_project,
        },
        tasks::{
            AddTaskError, AddTaskParameters, CompleteTaskError, CompleteTaskParameters, add_task,
            complete_task,
        },
    },
    storage::{Storage, json::JsonFileStorage},
};

mod models;
mod services;
mod storage;
mod ui;

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

    /// Show anytime tasks
    Anytime,

    /// Show someday tasks
    Someday,

    /// Show completed tasks (last 14 days)
    Logbook,

    /// Show deleted items
    Trash,

    /// Show all active tasks
    All,

    /// Add a new task
    Add {
        /// Task title
        title: String,

        /// Schedule for today
        #[arg(long)]
        today: bool,

        /// Schedule for today (evening)
        #[arg(long)]
        evening: bool,

        /// Defer to someday
        #[arg(long)]
        someday: bool,

        /// Available anytime (no specific date)
        #[arg(long)]
        anytime: bool,

        /// Schedule for a specific date (e.g., "friday", "2025-03-01")
        #[arg(short, long)]
        when: Option<String>,

        /// Set a hard deadline
        #[arg(short, long)]
        deadline: Option<String>,

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

    /// Moves a task
    Move {
        /// Task number
        task_number: String,

        /// Schedule for today
        #[arg(long)]
        today: bool,

        /// Schedule for today (evening)
        #[arg(long)]
        evening: bool,

        /// Defer to someday
        #[arg(long)]
        someday: bool,

        /// Available anytime (no specific date)
        #[arg(long)]
        anytime: bool,

        /// Schedule for a specific date (e.g., "friday", "2025-03-01")
        #[arg(short, long)]
        when: Option<String>,

        /// Set a hard deadline
        #[arg(short, long)]
        deadline: Option<String>,

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

    /// Complete a task
    Done { task_number_or_fuzzy_name: String },

    /// Manage areas
    #[command(subcommand)]
    Area(AreaCommands),

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Manage tags
    #[command(subcommand)]
    Tag(TagCommands),
}

#[derive(Debug, Subcommand)]
enum AreaCommands {
    /// Create a new area
    New {
        /// Name of the area
        name: String,
    },
    /// Delete an area
    Delete { name: String },
    /// List all areas
    List,
    /// View projects in an area
    View { slug: String },
}

#[derive(Debug, Subcommand)]
enum ProjectCommands {
    /// Create a new project
    New {
        /// Name of the project
        name: String,
        /// Assign to an area
        #[arg(short, long)]
        area: Option<String>,
    },
    /// Delete an project
    Delete { name: String },
    /// List all projects
    List,
    /// View tasks in a project
    View { slug: String },
}

#[derive(Debug, Subcommand)]
enum TagCommands {
    /// List all tags
    List,
    /// View tasks with a specific tag
    View { name: String },
}

fn main() {
    let cli = Cli::parse();

    // Initialize storage
    let storage_path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tdo")
        .join("store.json");

    // Create parent directory if it doesn't exist
    if let Some(parent) = storage_path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("Error: Failed to create data directory: {}", e);
            std::process::exit(1);
        });
    }

    let storage = JsonFileStorage::new(storage_path);

    let mut store = match storage.load() {
        Ok(store) => store,
        Err(e) => {
            eprintln!("Error: Failed to load store: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Today) => {
            let today = jiff::Zoned::now().date();

            // Collect today tasks
            let mut today_regular: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Today { evening: false }))
                .filter(|t| t.completed_at.is_none())
                .collect();

            let mut today_evening: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Today { evening: true }))
                .filter(|t| t.completed_at.is_none())
                .collect();

            // Collect overdue tasks
            let mut overdue_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    if let When::Scheduled { date } = t.when {
                        date < today && t.completed_at.is_none()
                    } else {
                        false
                    }
                })
                .collect();

            // Sort by task number
            today_regular.sort_by_key(|t| t.task_number);
            today_evening.sort_by_key(|t| t.task_number);
            overdue_tasks.sort_by_key(|t| t.task_number);

            let total = today_regular.len() + today_evening.len() + overdue_tasks.len();

            if total == 0 {
                println!("No tasks for today");
            } else {
                ui::render_view_header(&format!("Today ({})", today.strftime("%b %d")), total);

                // Show overdue first if any
                if !overdue_tasks.is_empty() {
                    ui::render_section_header("Overdue");
                    for task in overdue_tasks {
                        ui::render_task_line(task, &store, true);
                    }
                }

                // Show regular today tasks
                if !today_regular.is_empty() {
                    for task in today_regular {
                        ui::render_task_line(task, &store, false);
                    }
                }

                // Show evening tasks
                if !today_evening.is_empty() {
                    ui::render_section_header("Evening");
                    for task in today_evening {
                        ui::render_task_line(task, &store, false);
                    }
                }
            }
        }
        Some(Commands::Inbox) => {
            // Filter inbox tasks
            let inbox_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Inbox))
                .filter(|t| t.completed_at.is_none())
                .collect();

            // Display
            if inbox_tasks.is_empty() {
                println!("Inbox is empty");
            } else {
                ui::render_view_header("Inbox", inbox_tasks.len());
                for task in inbox_tasks {
                    ui::render_task_line(task, &store, false);
                }
            }
        }
        Some(Commands::Anytime) => {
            // Filter anytime tasks
            let anytime_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Anytime))
                .filter(|t| t.completed_at.is_none())
                .collect();

            // Display
            if anytime_tasks.is_empty() {
                println!("No anytime tasks");
            } else {
                ui::render_view_header("Anytime", anytime_tasks.len());
                for task in anytime_tasks {
                    ui::render_task_line(task, &store, false);
                }
            }
        }
        Some(Commands::Someday) => {
            // Filter someday tasks
            let someday_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Someday))
                .filter(|t| t.completed_at.is_none())
                .collect();

            // Display
            if someday_tasks.is_empty() {
                println!("No someday tasks");
            } else {
                ui::render_view_header("Someday", someday_tasks.len());
                for task in someday_tasks {
                    ui::render_task_line(task, &store, false);
                }
            }
        }
        Some(Commands::All) => {
            use std::collections::HashMap;

            // Collect all active, incomplete tasks
            let all_tasks: Vec<_> = store.get_active_tasks().collect();

            if all_tasks.is_empty() {
                println!("No active tasks");
            } else {
                // Group tasks by When variant
                let mut grouped: HashMap<String, Vec<&crate::models::task::Task>> = HashMap::new();

                for task in &all_tasks {
                    let group = match &task.when {
                        When::Inbox => "Inbox",
                        When::Today { evening: false } => "Today",
                        When::Today { evening: true } => "Today (Evening)",
                        When::Someday => "Someday",
                        When::Anytime => "Anytime",
                        When::Scheduled { date: _ } => "Scheduled",
                    };
                    grouped
                        .entry(group.to_string())
                        .or_insert_with(Vec::new)
                        .push(task);
                }

                // Display in a logical order
                let order = vec![
                    "Inbox",
                    "Today",
                    "Today (Evening)",
                    "Scheduled",
                    "Anytime",
                    "Someday",
                ];

                for group_name in order {
                    if let Some(tasks) = grouped.get(group_name) {
                        ui::render_section_header(group_name);
                        for task in tasks {
                            let is_overdue = ui::is_overdue(task);
                            ui::render_task_line(task, &store, is_overdue);
                        }
                    }
                }
            }
        }
        Some(Commands::Upcoming) => {
            use jiff::civil::Date;
            use std::collections::BTreeMap;

            let today = jiff::Zoned::now().date();

            // Collect upcoming tasks (scheduled in the future)
            let upcoming_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    if let When::Scheduled { date } = t.when {
                        date > today && t.completed_at.is_none()
                    } else {
                        false
                    }
                })
                .collect();

            if upcoming_tasks.is_empty() {
                println!("No upcoming tasks");
            } else {
                // Group by date
                let mut grouped: BTreeMap<Date, Vec<&crate::models::task::Task>> = BTreeMap::new();

                for task in &upcoming_tasks {
                    if let When::Scheduled { date } = task.when {
                        grouped.entry(date).or_insert_with(Vec::new).push(task);
                    }
                }

                ui::render_view_header("Upcoming", upcoming_tasks.len());

                // Display by date
                for (date, mut tasks) in grouped {
                    tasks.sort_by_key(|t| t.task_number);
                    ui::render_section_header(&ui::format_date_header(date));
                    for task in tasks {
                        ui::render_task_line(task, &store, false);
                    }
                }
            }
        }
        Some(Commands::Logbook) => {
            use std::collections::BTreeMap;

            // Collect completed tasks from last 14 days
            let completed_tasks: Vec<_> = store
                .tasks
                .values()
                .filter(|t| {
                    if let Some(completed_at) = t.completed_at {
                        ui::is_within_days(completed_at, 14)
                    } else {
                        false
                    }
                })
                .collect();

            if completed_tasks.is_empty() {
                println!("No completed tasks in the last 14 days");
            } else {
                // Group by month
                let mut grouped: BTreeMap<(i16, i8), Vec<&crate::models::task::Task>> =
                    BTreeMap::new();

                for task in &completed_tasks {
                    if let Some(completed_at) = task.completed_at {
                        let year_month = ui::get_year_month(completed_at);
                        grouped
                            .entry(year_month)
                            .or_insert_with(Vec::new)
                            .push(task);
                    }
                }

                ui::render_view_header("Logbook", completed_tasks.len());

                // Display by month (most recent first)
                for (_year_month, tasks) in grouped.iter().rev() {
                    // Sort tasks within month by completion time (most recent first)
                    let mut sorted_tasks = tasks.clone();
                    sorted_tasks
                        .sort_by(|a, b| b.completed_at.unwrap().cmp(&a.completed_at.unwrap()));

                    // Use the first task's timestamp to format the month header
                    let month_header =
                        ui::format_month_header(sorted_tasks[0].completed_at.unwrap());
                    ui::render_section_header(&month_header);

                    for task in sorted_tasks {
                        ui::render_task_line_with_completion_date(task, &store, false);
                    }
                }
            }
        }
        Some(Commands::Trash) => {
            // Collect deleted items
            let deleted_tasks: Vec<_> = store.get_deleted_tasks().collect();
            let deleted_projects: Vec<_> = store.get_deleted_projects().collect();
            let deleted_areas: Vec<_> = store.get_deleted_areas().collect();

            let total = deleted_tasks.len() + deleted_projects.len() + deleted_areas.len();

            if total == 0 {
                println!("Trash is empty");
            } else {
                ui::render_view_header("Trash", total);

                // Show deleted tasks
                if !deleted_tasks.is_empty() {
                    ui::render_section_header(&format!("Tasks ({})", deleted_tasks.len()));
                    for task in deleted_tasks {
                        ui::render_task_line(task, &store, false);
                    }
                }

                // Show deleted projects
                if !deleted_projects.is_empty() {
                    ui::render_section_header(&format!("Projects ({})", deleted_projects.len()));
                    for project in deleted_projects {
                        println!("  {} {}", "•".dimmed(), project.name.dimmed());
                    }
                }

                // Show deleted areas
                if !deleted_areas.is_empty() {
                    ui::render_section_header(&format!("Areas ({})", deleted_areas.len()));
                    for area in deleted_areas {
                        println!("  {} {}", "•".dimmed(), area.name.dimmed());
                    }
                }
            }
        }
        Some(Commands::Add {
            title,
            today,
            evening,
            someday,
            anytime,
            when: when_str,
            deadline,
            project,
            area,
            tag,
            notes,
        }) => {
            // Parse when flags
            let when = match When::from_command_flags(today, evening, someday, anytime, when_str) {
                Ok(w) => w,
                Err(WhenInstantiationError::ScheduleAtIncorrect(date_str)) => {
                    eprintln!("Error: Invalid schedule date format: '{}'", date_str);
                    eprintln!(
                        "\nExpected format: YYYY-MM-DD (e.g., 2025-03-01) or relative dates like 'friday', 'next monday'"
                    );
                    std::process::exit(1);
                }
                Err(WhenInstantiationError::ConflictingFlags(flags)) => {
                    eprintln!("Error: Cannot use multiple scheduling flags together");
                    eprintln!("\nConflicting flags provided: {}", flags.join(", "));
                    eprintln!("\nPlease use only one of:");
                    eprintln!("  --today       Schedule for today");
                    eprintln!("  --someday     Defer to someday");
                    eprintln!("  --anytime     Available anytime");
                    eprintln!("  --when DATE   Schedule for a specific date");
                    std::process::exit(1);
                }
                Err(WhenInstantiationError::EveningWithoutToday) => {
                    eprintln!("Error: The --evening flag can only be used with --today");
                    eprintln!("\nExample: tdo add 'Review PRs' --today --evening");
                    std::process::exit(1);
                }
            };

            // Build parameters
            let params = AddTaskParameters {
                title: title.clone(),
                notes,
                when,
                deadline,
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
                    eprintln!("\nExpected format: YYYY-MM-DD (e.g., 2025-03-01)");
                    std::process::exit(1);
                }
                Err(AddTaskError::Storage(e)) => {
                    eprintln!("Error: Failed to save task: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Done {
            task_number_or_fuzzy_name,
        }) => {
            // Build parameters
            let params = CompleteTaskParameters {
                task_number_or_fuzzy_name,
            };

            // Call service
            match complete_task(&mut store, &storage, params) {
                Ok(task) => {
                    println!("✓ Task completed: {}", task.title);
                    println!("  #{}", task.task_number);
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
        Some(Commands::Move {
            task_number,
            today,
            evening,
            someday,
            anytime,
            when,
            deadline,
            project,
            area,
            tag,
            notes,
        }) => {
            todo!()
        }
        Some(Commands::Area(AreaCommands::New { name })) => {
            let params = CreateAreaParameters { name };
            match create_area(&mut store, &storage, params) {
                Ok(area) => {
                    println!("✓ Area {} created with slug {}", area.name, area.slug);
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
        Some(Commands::Area(AreaCommands::Delete { name })) => {
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
        Some(Commands::Area(AreaCommands::List)) => {
            // Collect all active areas
            let mut areas: Vec<_> = store.get_active_areas().collect();

            if areas.is_empty() {
                println!("No areas found");
            } else {
                // Sort alphabetically by name (case-insensitive)
                areas.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                println!(
                    "{} ({} {})\n",
                    "AREAS".cyan(),
                    areas.len(),
                    if areas.len() == 1 { "area" } else { "areas" }
                );

                for area in areas {
                    // Count active projects in this area
                    let project_count = store
                        .get_projects_for_area(area.id)
                        .filter(|p| p.deleted_at.is_none())
                        .count();

                    // Count active tasks - includes both direct tasks and tasks within projects
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

                    // Display area name
                    println!("{} {}", "•".green(), area.name.bold());

                    // Display counts
                    println!(
                        "    {} {} {} {}",
                        project_count.to_string().dimmed(),
                        if project_count == 1 {
                            "project"
                        } else {
                            "projects"
                        },
                        "•".dimmed(),
                        format!(
                            "{} {}",
                            total_task_count,
                            if total_task_count == 1 {
                                "task"
                            } else {
                                "tasks"
                            }
                        )
                        .dimmed()
                    );

                    // Display separator
                    println!("    {}", "─".repeat(30).dimmed());
                    println!();
                }
            }
        }
        Some(Commands::Project(ProjectCommands::New { name, area })) => {
            let params = CreateProjectParameters { name, area };
            match create_project(&mut store, &storage, params) {
                Ok(project) => {
                    println!(
                        "✓ Project {} created with slug {}",
                        project.name, project.slug
                    );
                }
                Err(CreateProjectError::AreaNotFound(area)) => {
                    eprintln!("Error: Area with name '{}' not found", area);
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
        Some(Commands::Project(ProjectCommands::Delete { name })) => {
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
        Some(Commands::Project(ProjectCommands::List)) => {
            // Collect all active projects
            let mut projects: Vec<_> = store.get_active_projects().collect();

            if projects.is_empty() {
                println!("No projects found");
            } else {
                // Sort alphabetically by name (case-insensitive)
                projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                println!(
                    "{} ({} {})\n",
                    "PROJECTS".cyan(),
                    projects.len(),
                    if projects.len() == 1 {
                        "project"
                    } else {
                        "projects"
                    }
                );

                for project in projects {
                    // Count active tasks in this project
                    let task_count = store
                        .get_tasks_for_project(project.id)
                        .filter(|t| t.deleted_at.is_none())
                        .count();

                    // Display project name
                    println!("{} {}", "•".green(), project.name.bold());

                    // Display area if project belongs to one
                    if let Some(area_id) = project.area_id {
                        if let Some(area) = store.get_area(area_id) {
                            println!("    {} {}", "Area:".dimmed(), area.name.blue());
                        }
                    }

                    // Display task count
                    println!(
                        "    {} {}",
                        task_count.to_string().dimmed(),
                        if task_count == 1 { "task" } else { "tasks" }.dimmed()
                    );

                    // Display separator
                    println!("    {}", "─".repeat(30).dimmed());
                    println!();
                }
            }
        }
        Some(Commands::Project(ProjectCommands::View { slug })) => {
            // Find project by slug (case-insensitive)
            let project = store
                .get_active_projects()
                .find(|p| p.slug.to_lowercase() == slug.to_lowercase());

            match project {
                None => {
                    eprintln!("Error: Project '{}' not found", slug);

                    let projects: Vec<_> = store.get_active_projects().collect();
                    if !projects.is_empty() {
                        eprintln!("\nAvailable projects:");
                        for p in projects {
                            eprintln!("  - {} ({})", p.name, p.slug);
                        }
                    }
                    std::process::exit(1);
                }
                Some(project) => {
                    // Get tasks for this project
                    let mut tasks: Vec<_> = store
                        .get_tasks_for_project(project.id)
                        .filter(|t| t.completed_at.is_none() && t.deleted_at.is_none())
                        .collect();

                    tasks.sort_by_key(|t| t.task_number);

                    // Display header with project name and area if applicable
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
                        ui::render_view_header(&header, tasks.len());
                        for task in tasks {
                            let is_overdue = ui::is_overdue(task);
                            ui::render_task_line(task, &store, is_overdue);
                        }
                    }
                }
            }
        }
        Some(Commands::Area(AreaCommands::View { slug })) => {
            // Find area by slug (case-insensitive)
            let area = store
                .get_active_areas()
                .find(|a| a.slug.to_lowercase() == slug.to_lowercase());

            match area {
                None => {
                    eprintln!("Error: Area '{}' not found", slug);

                    let areas: Vec<_> = store.get_active_areas().collect();
                    if !areas.is_empty() {
                        eprintln!("\nAvailable areas:");
                        for a in areas {
                            eprintln!("  - {} ({})", a.name, a.slug);
                        }
                    }
                    std::process::exit(1);
                }
                Some(area) => {
                    // Get projects in this area
                    let mut projects: Vec<_> = store
                        .get_projects_for_area(area.id)
                        .filter(|p| p.deleted_at.is_none())
                        .collect();

                    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    if projects.is_empty() {
                        println!("No projects in area '{}'", area.name);
                    } else {
                        println!(
                            "\n  {} ({} {})\n",
                            area.name.cyan().bold(),
                            projects.len(),
                            if projects.len() == 1 {
                                "project"
                            } else {
                                "projects"
                            }
                        );

                        for project in projects {
                            // Count active tasks in this project
                            let task_count = store
                                .get_tasks_for_project(project.id)
                                .filter(|t| t.deleted_at.is_none() && t.completed_at.is_none())
                                .count();

                            println!("  {} {}", "•".green(), project.name.bold());
                            println!(
                                "    {} {}",
                                task_count.to_string().dimmed(),
                                if task_count == 1 { "task" } else { "tasks" }.dimmed()
                            );
                            println!();
                        }
                    }
                }
            }
        }
        Some(Commands::Tag(TagCommands::List)) => {
            // Collect all unique tags from active tasks
            use std::collections::HashMap;

            let mut tag_counts: HashMap<String, usize> = HashMap::new();

            for task in store
                .get_active_tasks()
                .filter(|t| t.completed_at.is_none())
            {
                for tag in &task.tags {
                    *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }

            if tag_counts.is_empty() {
                println!("No tags found");
            } else {
                let mut tags: Vec<_> = tag_counts.iter().collect();
                tags.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

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
        }
        Some(Commands::Tag(TagCommands::View { name })) => {
            // Find tasks with this tag (case-insensitive)
            let mut tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    t.completed_at.is_none()
                        && t.tags
                            .iter()
                            .any(|tag| tag.to_lowercase() == name.to_lowercase())
                })
                .collect();

            if tasks.is_empty() {
                println!("No tasks with tag '{}'", name);

                // Suggest available tags
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
                tasks.sort_by_key(|t| t.task_number);
                ui::render_view_header(&format!("#{}", name), tasks.len());
                for task in tasks {
                    let is_overdue = ui::is_overdue(task);
                    ui::render_task_line(task, &store, is_overdue);
                }
            }
        }
        None => {
            // Default: show today view (same as `tdo today`)
            let today = jiff::Zoned::now().date();

            // Collect today tasks
            let mut today_regular: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Today { evening: false }))
                .filter(|t| t.completed_at.is_none())
                .collect();

            let mut today_evening: Vec<_> = store
                .get_active_tasks()
                .filter(|t| matches!(t.when, When::Today { evening: true }))
                .filter(|t| t.completed_at.is_none())
                .collect();

            // Collect overdue tasks
            let mut overdue_tasks: Vec<_> = store
                .get_active_tasks()
                .filter(|t| {
                    if let When::Scheduled { date } = t.when {
                        date < today && t.completed_at.is_none()
                    } else {
                        false
                    }
                })
                .collect();

            // Sort by task number
            today_regular.sort_by_key(|t| t.task_number);
            today_evening.sort_by_key(|t| t.task_number);
            overdue_tasks.sort_by_key(|t| t.task_number);

            let total = today_regular.len() + today_evening.len() + overdue_tasks.len();

            if total == 0 {
                println!("No tasks for today");
            } else {
                ui::render_view_header(&format!("Today ({})", today.strftime("%b %d")), total);

                // Show overdue first if any
                if !overdue_tasks.is_empty() {
                    ui::render_section_header("Overdue");
                    for task in overdue_tasks {
                        ui::render_task_line(task, &store, true);
                    }
                }

                // Show regular today tasks
                if !today_regular.is_empty() {
                    for task in today_regular {
                        ui::render_task_line(task, &store, false);
                    }
                }

                // Show evening tasks
                if !today_evening.is_empty() {
                    ui::render_section_header("Evening");
                    for task in today_evening {
                        ui::render_task_line(task, &store, false);
                    }
                }
            }
        }
    }
}
