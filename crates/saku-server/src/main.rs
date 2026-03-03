mod auth;
mod cli;
mod config;
mod db;
mod error;
mod kv;
mod state;
mod sync;

use axum::{Json, Router, routing::get};
use clap::Parser;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::cli::{Cli, Commands};
use crate::state::AppState;

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
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/sync", sync::router())
        .nest("/api/v1/kv", kv::router())
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}
