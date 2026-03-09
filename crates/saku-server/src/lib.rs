pub mod auth;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod kv;
pub mod state;
pub mod sync;

use axum::{Json, Router, routing::get};
use state::AppState;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

/// Build the application router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/sync", sync::router())
        .nest("/api/v1/kv", kv::router())
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}
