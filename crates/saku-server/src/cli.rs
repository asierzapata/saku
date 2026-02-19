use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "saku-server",
    about = "Auth and coordination server for saku sync"
)]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the server (default)
    Serve,

    /// Create a new user account
    CreateUser {
        /// User's email address
        #[arg(long)]
        email: String,
    },
}

/// Run the create-user CLI command: prompts for password, inserts into DB.
pub fn run_create_user(config_path: &std::path::Path, email: &str) -> anyhow::Result<()> {
    let config = crate::config::load_config(config_path)?;

    // Prompt for password
    let password = rpassword::prompt_password("Password: ")?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }
    let confirm = rpassword::prompt_password("Confirm password: ")?;
    if password != confirm {
        anyhow::bail!("Passwords do not match");
    }

    // Hash password
    let hash = crate::auth::password::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?;

    // Open DB and run migrations
    let conn = rusqlite::Connection::open(&config.database.path)?;
    crate::db::migrations::run_migrations(&conn)?;

    // Create user
    let user_id = crate::db::users::create_user(&conn, email, &hash)?;
    println!("User created: {} (id: {})", email, user_id);

    Ok(())
}
