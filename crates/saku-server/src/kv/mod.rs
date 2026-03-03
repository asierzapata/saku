pub mod handlers;

use crate::state::AppState;
use axum::{
    Router,
    routing::{get, put},
};

/// Build the KV router: `/api/v1/kv/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/:tool", get(handlers::get_entries).put(handlers::batch_put))
        .route("/:tool/snapshot", get(handlers::snapshot))
        .route("/:tool/:key", put(handlers::put_single))
}
