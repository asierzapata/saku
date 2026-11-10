pub mod handlers;

use crate::state::AppState;
use axum::{
    Router,
    routing::{get, put},
};

/// Build the KV router: `/api/v1/kv/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/:tool", get(handlers::get_entries).put(handlers::reject_legacy_write))
        .route("/:tool/snapshot", get(handlers::snapshot).put(handlers::reject_legacy_write))
        .route("/:tool/:key", put(handlers::reject_legacy_write))
}

pub fn v2_router() -> Router<AppState> {
    Router::new()
        .route("/:tool", get(handlers::get_entries_v2).put(handlers::batch_put))
        .route("/:tool/snapshot", get(handlers::snapshot_v2))
}
