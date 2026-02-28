use std::path::PathBuf;
use std::time::Duration;

use saku_tdo::storage::Storage;
use saku_tdo::storage::json::JsonFileStorage;
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::executor;
use crate::logs;
use crate::picker;
use crate::prompt;
use crate::reporter;

#[derive(Debug)]
pub struct DaemonConfig {
    pub poll_interval: Duration,
    pub max_concurrent: usize,
    pub once: bool,
    pub working_dir: PathBuf,
    pub storage_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Failed to load store: {0}")]
    StoreLoadFailed(String),
}

/// Run the daemon loop: poll for tasks, claim, execute, report.
pub async fn run_daemon(config: DaemonConfig) -> Result<(), DaemonError> {
    let semaphore = std::sync::Arc::new(Semaphore::new(config.max_concurrent));

    // Read project CLAUDE.md once (if present)
    let claude_md_path = config.working_dir.join("CLAUDE.md");
    let project_claude_md = std::fs::read_to_string(&claude_md_path).ok();

    loop {
        // Load store fresh each cycle
        let storage = JsonFileStorage::new(config.storage_path.clone());
        let mut store = match storage.load() {
            Ok(store) => store,
            Err(e) => {
                eprintln!("Warning: Failed to load store: {}", e);
                if config.once {
                    return Err(DaemonError::StoreLoadFailed(e.to_string()));
                }
                tokio::time::sleep(config.poll_interval).await;
                continue;
            }
        };

        // Collect task numbers + titles upfront to release borrow on store
        let executable: Vec<(u64, String)> = picker::pick_executable_tasks(&store)
            .into_iter()
            .map(|t| (t.task_number, t.title.clone()))
            .collect();

        if executable.is_empty() {
            if config.once {
                println!("No executable tasks found.");
                return Ok(());
            }
            tokio::time::sleep(config.poll_interval).await;
            continue;
        }

        // Process tasks up to concurrency limit
        let mut handles = vec![];

        for (task_number, task_title) in executable {
            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => break, // At concurrency limit
            };
            let working_dir = config.working_dir.clone();
            let storage_path = config.storage_path.clone();
            let claude_md = project_claude_md.clone();

            // Claim the task before spawning
            match reporter::claim_task(&mut store, &storage, task_number) {
                Ok(_) => {
                    println!("→ Claimed task #{}: {}", task_number, task_title);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to claim task #{}: {}",
                        task_number, e
                    );
                    drop(permit);
                    continue;
                }
            }

            // Build prompt synchronously (needs store reference)
            let task_ref = store.get_task_by_number(task_number).unwrap();
            let task_prompt = prompt::build_prompt(task_ref, &store, claude_md.as_deref());

            let handle = tokio::spawn(async move {
                let _permit = permit;

                println!("▶ Executing task #{}: {}", task_number, task_title);

                match executor::execute_task(&task_prompt, &working_dir, None).await {
                    Ok(result) => {
                        // Write log
                        match logs::write_log(task_number, &result) {
                            Ok(path) => {
                                println!("  Log: {}", path.display());
                            }
                            Err(e) => {
                                eprintln!("  Warning: Failed to write log: {}", e);
                            }
                        }

                        // Report result
                        let storage = JsonFileStorage::new(storage_path);
                        let mut store = match storage.load() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!(
                                    "  Error: Failed to reload store for task #{}: {}",
                                    task_number, e
                                );
                                return;
                            }
                        };

                        if result.exit_code == 0 {
                            let summary = truncate_output(&result.stdout, 500);
                            match reporter::report_success(
                                &mut store,
                                &storage,
                                task_number,
                                &summary,
                            ) {
                                Ok(_) => {
                                    println!(
                                        "✓ Task #{} completed: {}",
                                        task_number, task_title
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "  Error: Failed to report success for #{}: {}",
                                        task_number, e
                                    );
                                }
                            }
                        } else {
                            let error = format!(
                                "Exit code: {}\n{}",
                                result.exit_code,
                                truncate_output(&result.stderr, 500)
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
                                        task_number, result.exit_code, task_title
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "  Error: Failed to report failure for #{}: {}",
                                        task_number, e
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Task #{} execution error: {}", task_number, e);
                        let storage = JsonFileStorage::new(storage_path);
                        if let Ok(mut store) = storage.load() {
                            let _ = reporter::report_failure(
                                &mut store,
                                &storage,
                                task_number,
                                &e.to_string(),
                            );
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all spawned tasks to complete
        for handle in handles {
            let _ = handle.await;
        }

        if config.once {
            return Ok(());
        }

        tokio::time::sleep(config.poll_interval).await;
    }
}

fn truncate_output(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        format!("{}...(truncated)", &s[..max_chars])
    }
}
