use clap::Parser;

use saku_server::cli;
use saku_server::cli::{Cli, Commands};
use saku_server::config;
use saku_server::db;
use saku_server::state::AppState;
use saku_server::sync;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "saku_server=info,tower_http=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::CreateUser { email }) => {
            cli::run_create_user(&cli.config, &email)?;
            return Ok(());
        }
        Some(Commands::Serve) | None => {
            // Continue to server startup
        }
    }

    // Load config
    let config = config::load_config(&cli.config)?;
    tracing::info!("Loaded config from {:?}", cli.config);

    // Open database and run migrations
    let conn = rusqlite::Connection::open(&config.database.path)?;
    db::migrations::run_migrations(&conn)?;
    tracing::info!("Database ready at {:?}", config.database.path);

    // Build storage operator
    let storage = sync::storage::build_operator(&config.storage)?;
    tracing::info!("Storage operator ready (bucket: {})", config.storage.bucket);

    // Build app state
    let state = AppState::new(conn, storage, config.clone());

    // Build router
    let app = saku_server::build_router(state);

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
