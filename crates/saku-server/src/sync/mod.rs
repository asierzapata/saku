pub mod handlers;
pub mod storage;

use axum::{Router, routing::{get, post}};
use crate::state::AppState;

/// Build the sync router: `/api/v1/sync/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{tool}/download-url", get(handlers::download_url))
        .route("/{tool}/upload-url", post(handlers::upload_url))
        .route("/{tool}/confirm-upload", post(handlers::confirm_upload))
        .route("/{tool}/metadata", get(handlers::metadata))
}
