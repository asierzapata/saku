use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use saku_tdo::storage::Storage;
use saku_tdo::storage::json::JsonFileStorage;

use saku_wrk::{daemon, executor, logs, prompt, reporter};

#[derive(Parser)]
#[command(name = "wrk", about = "Agent task executor for the saku productivity suite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a single task immediately
    Run {
        /// Task number to execute
        task_number: u64,

        /// Show the prompt that would be sent to claude, without executing
        #[arg(long)]
        dry: bool,

        /// Mark task as "needs review" instead of "done" on success
        #[arg(long)]
        review: bool,

        /// Working directory for execution
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
    },

    /// Long-running process that polls for agent-assigned tasks
    Daemon {
        /// Poll interval (e.g., "30s", "5m")
        #[arg(long, default_value = "60s", value_parser = parse_duration)]
        poll: Duration,

        /// Run a single poll cycle then exit
        #[arg(long)]
        once: bool,

        /// Maximum number of concurrent task executions
        #[arg(long, default_value = "1")]
        max_concurrent: usize,

        /// Working directory for execution
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
    },

    /// Show current/recent execution state
    Status {
        /// Show execution history
        #[arg(long)]
        history: bool,
    },

    /// View the full execution log for a task
    Log {
        /// Task number
        task_number: u64,
    },
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| format!("Invalid duration: {}", e))
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>()
            .map(|m| Duration::from_secs(m * 60))
            .map_err(|e| format!("Invalid duration: {}", e))
    } else {
        s.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("Invalid duration '{}'. Use e.g. '30s' or '5m'", s))
    }
}

fn load_store(storage_path: &PathBuf) -> (JsonFileStorage, saku_tdo::models::store::Store) {
    // Create parent directory if needed
    if let Some(parent) = storage_path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("Error: Failed to create data directory: {}", e);
            std::process::exit(1);
        });
    }

    let storage = JsonFileStorage::new(storage_path.clone());
    let store = match storage.load() {
        Ok(store) => store,
        Err(e) => {
            eprintln!("Error: Failed to load tdo store: {}", e);
            std::process::exit(1);
        }
    };
    (storage, store)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let storage_path = saku_tdo::storage::default_storage_path();

    match cli.command {
        Commands::Run {
            task_number,
            dry,
            review,
            dir,
        } => {
            let working_dir = dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let (storage, mut store) = load_store(&storage_path);

            // Look up task
            let task = match store.get_task_by_number(task_number) {
                Some(t) => t.clone(),
                None => {
                    eprintln!("Error: Task #{} not found", task_number);
                    std::process::exit(1);
                }
            };

            if task.completed_at.is_some() {
                eprintln!("Error: Task #{} is already completed", task_number);
                std::process::exit(1);
            }

            if task.deleted_at.is_some() {
                eprintln!("Error: Task #{} is deleted", task_number);
                std::process::exit(1);
            }

            // Read project CLAUDE.md if present
            let claude_md_path = working_dir.join("CLAUDE.md");
            let project_claude_md = std::fs::read_to_string(&claude_md_path).ok();

            // Build prompt
            let task_prompt =
                prompt::build_prompt(&task, &store, project_claude_md.as_deref());

            if dry {
                println!("{}", task_prompt);
                return;
            }

            // Claim
            match reporter::claim_task(&mut store, &storage, task_number) {
                Ok(_) => println!("→ Claimed task #{}: {}", task_number, task.title),
                Err(e) => {
                    eprintln!("Error: Failed to claim task: {}", e);
                    std::process::exit(1);
                }
            }

            // Execute
            println!("▶ Executing task #{}...", task_number);
            match executor::execute_task(&task_prompt, &working_dir, None).await {
                Ok(result) => {
                    // Write log
                    match logs::write_log(task_number, &result) {
                        Ok(path) => println!("  Log: {}", path.display()),
                        Err(e) => eprintln!("  Warning: Failed to write log: {}", e),
                    }

                    // Reload store (in case anything changed during execution)
                    let (storage, mut store) = load_store(&storage_path);

                    if result.exit_code == 0 {
                        let summary = if result.stdout.len() > 500 {
                            format!("{}...(truncated)", &result.stdout[..500])
                        } else {
                            result.stdout.clone()
                        };

                        if review {
                            match reporter::report_needs_review(
                                &mut store,
                                &storage,
                                task_number,
                                &summary,
                            ) {
                                Ok(_) => {
                                    println!(
                                        "⏸ Task #{} needs review: {}",
                                        task_number, task.title
                                    );
                                }
                                Err(e) => {
                                    eprintln!("Error: Failed to report review: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            match reporter::report_success(
                                &mut store,
                                &storage,
                                task_number,
                                &summary,
                            ) {
                                Ok(_) => {
                                    println!(
                                        "✓ Task #{} completed: {}",
                                        task_number, task.title
                                    );
                                }
                                Err(e) => {
                                    eprintln!("Error: Failed to report success: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    } else {
                        let error = format!(
                            "Exit code: {}\n{}",
                            result.exit_code,
                            if result.stderr.len() > 500 {
                                format!("{}...(truncated)", &result.stderr[..500])
                            } else {
                                result.stderr.clone()
                            }
                        );
                        match reporter::report_failure(
                            &mut store,
                            &storage,
                            task_number,
                            &error,
                        ) {
                            Ok(_) => {
                                eprintln!(
                                    "✗ Task #{} failed (exit {}): {}",
                                    task_number, result.exit_code, task.title
                                );
                            }
                            Err(e) => {
                                eprintln!("Error: Failed to report failure: {}", e);
                            }
                        }
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error: Execution failed: {}", e);

                    // Report failure to tdo
                    let (storage, mut store) = load_store(&storage_path);
                    let _ = reporter::report_failure(
                        &mut store,
                        &storage,
                        task_number,
                        &e.to_string(),
                    );

                    std::process::exit(1);
                }
            }
        }

        Commands::Daemon {
            poll,
            once,
            max_concurrent,
            dir,
        } => {
            let working_dir = dir.unwrap_or_else(|| std::env::current_dir().unwrap());

            let config = daemon::DaemonConfig {
                poll_interval: poll,
                max_concurrent,
                once,
                working_dir,
                storage_path,
            };

            if !once {
                println!(
                    "wrk daemon started (poll: {:?}, max concurrent: {})",
                    poll, max_concurrent
                );
            }

            if let Err(e) = daemon::run_daemon(config).await {
                eprintln!("Error: Daemon failed: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Status { history } => {
            let entries = match logs::list_logs() {
                Ok(entries) => entries,
                Err(e) => {
                    eprintln!("Error: Failed to read logs: {}", e);
                    std::process::exit(1);
                }
            };

            if entries.is_empty() {
                println!("No execution history.");
                return;
            }

            let count = if history { 20 } else { 5 };
            let (_, store) = load_store(&storage_path);

            println!("{:<8} {:<30} {:<20}", "Task", "Title", "Timestamp");
            println!("{}", "-".repeat(60));

            for entry in entries.iter().take(count) {
                let title = store
                    .get_task_by_number(entry.task_number)
                    .map(|t| {
                        if t.title.len() > 28 {
                            format!("{}...", &t.title[..25])
                        } else {
                            t.title.clone()
                        }
                    })
                    .unwrap_or_else(|| "(unknown)".to_string());

                println!(
                    "#{:<7} {:<30} {}",
                    entry.task_number, title, entry.timestamp
                );
            }
        }

        Commands::Log { task_number } => match logs::read_log(task_number) {
            Ok(content) => print!("{}", content),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    }
}
