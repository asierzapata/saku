pub mod handlers;
pub mod storage;

use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

/// Build the sync router: `/api/v1/sync/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{tool}/download-url", get(handlers::download_url))
        .route("/{tool}/upload-url", post(handlers::upload_url))
        .route("/{tool}/confirm-upload", post(handlers::confirm_upload))
        .route("/{tool}/metadata", get(handlers::metadata))
}
