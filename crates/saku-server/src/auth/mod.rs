pub mod handlers;
pub mod jwt;
pub mod middleware;
pub mod password;

use axum::{Router, routing::{post, delete}};
use crate::state::AppState;

/// Build the auth router: `/api/v1/auth/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/refresh", post(handlers::refresh))
        .route("/devices/{device_id}", delete(handlers::delete_device))
}
