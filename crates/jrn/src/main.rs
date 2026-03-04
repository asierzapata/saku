use clap::{Parser, Subcommand};
use colored::*;

use saku_jrn::{
    models::entry::EntryKind,
    services::entries::{AddEntryParameters, add_entry},
    storage::{Storage, json::JsonFileStorage},
};

#[derive(Parser)]
#[command(name = "jrn", about = "Daily journal for the human-agent team")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Log a journal entry
    Log {
        /// Entry content
        message: String,

        /// Assign to a project
        #[arg(short, long)]
        project: Option<String>,

        /// Reference a tdo task by number (adds tdo:{N} ref)
        #[arg(long, value_name = "TASK_NUMBER")]
        task: Option<u64>,

        /// Add tags (can be used multiple times)
        #[arg(short, long, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Add a cross-tool reference (can be used multiple times)
        #[arg(long = "ref", action = clap::ArgAction::Append)]
        refs: Vec<String>,
    },

    /// View journal entries
    View {
        /// What to view (today)
        #[arg(default_value = "today")]
        target: String,

        /// Output as JSON
        #[arg(long, short = 'j', conflicts_with = "toon")]
        json: bool,

        /// Output as TOON (token-efficient format for LLMs)
        #[arg(long, short = 't', conflicts_with = "json")]
        toon: bool,
    },

    /// Show a single entry in detail
    Show {
        /// Entry number
        id: u64,

        /// Output as JSON
        #[arg(long, short = 'j', conflicts_with = "toon")]
        json: bool,

        /// Output as TOON (token-efficient format for LLMs)
        #[arg(long, short = 't', conflicts_with = "json")]
        toon: bool,
    },

    /// Write or read a handoff entry
    Handoff {
        /// Handoff message (omit with --read to view latest handoff)
        message: Option<String>,

        /// Display the most recent handoff instead of writing
        #[arg(long)]
        read: bool,

        /// Assign to a project
        #[arg(short, long)]
        project: Option<String>,

        /// Reference a tdo task by number
        #[arg(long, value_name = "TASK_NUMBER")]
        task: Option<u64>,

        /// Add tags
        #[arg(short, long, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Add a cross-tool reference
        #[arg(long = "ref", action = clap::ArgAction::Append)]
        refs: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Ensure device_id exists
    if let Err(e) = saku_storage::device::get_or_create_device_id() {
        eprintln!("Warning: Failed to initialize device ID: {}", e);
    }

    // Initialize storage
    let storage_path = saku_jrn::storage::default_storage_path();
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
        // No subcommand = `jrn view today`
        None => {
            saku_jrn::ui::render_today_view(&store);
        }

        Some(Commands::Log {
            message,
            project,
            task,
            tag,
            refs,
        }) => {
            let mut all_refs = refs;
            if let Some(task_num) = task {
                all_refs.push(format!("tdo:{}", task_num));
            }

            let params = AddEntryParameters {
                body: message,
                kind: EntryKind::Log,
                project,
                tags: tag,
                refs: all_refs,
            };

            match add_entry(&mut store, &storage, params) {
                Ok(entry) => {
                    println!("#{}", entry.entry_number);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::View { target, json, toon }) => match target.as_str() {
            "today" => {
                if json {
                    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
                    let entries = store.get_entries_for_date(&today);
                    println!("{}", serde_json::to_string_pretty(&entries).unwrap());
                } else if toon {
                    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
                    let entries = store.get_entries_for_date(&today);
                    println!("{}", toon_format::encode::encode_default(&entries).unwrap());
                } else {
                    saku_jrn::ui::render_today_view(&store);
                }
            }
            other => {
                eprintln!("Unknown view target: '{}'", other);
                eprintln!("Available targets: today");
                std::process::exit(1);
            }
        },

        Some(Commands::Show { id, json, toon }) => match store.get_entry_by_number(id) {
            Some(entry) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(entry).unwrap());
                } else if toon {
                    println!("{}", toon_format::encode::encode_default(entry).unwrap());
                } else {
                    saku_jrn::ui::render_entry_detail(entry);
                }
            }
            None => {
                eprintln!("Entry #{} not found", id);
                std::process::exit(1);
            }
        },

        Some(Commands::Handoff {
            message,
            read,
            project,
            task,
            tag,
            refs,
        }) => {
            if read {
                match store.get_latest_handoff() {
                    Some(entry) => {
                        saku_jrn::ui::render_handoff_read(entry);
                    }
                    None => {
                        println!("No handoff entries found.");
                    }
                }
            } else {
                let body = match message {
                    Some(msg) => msg,
                    None => {
                        eprintln!(
                            "Error: message required (or use --read to view latest handoff)"
                        );
                        std::process::exit(1);
                    }
                };

                let mut all_refs = refs;
                if let Some(task_num) = task {
                    all_refs.push(format!("tdo:{}", task_num));
                }

                let params = AddEntryParameters {
                    body,
                    kind: EntryKind::Handoff,
                    project,
                    tags: tag,
                    refs: all_refs,
                };

                match add_entry(&mut store, &storage, params) {
                    Ok(entry) => {
                        println!("{} Handoff #{}", "★".yellow().bold(), entry.entry_number);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
