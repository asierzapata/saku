pub mod handlers;
pub mod jwt;
pub mod middleware;
pub mod password;

use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, post},
};

/// Build the auth router: `/api/v1/auth/*`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/refresh", post(handlers::refresh))
        .route("/devices/{device_id}", delete(handlers::delete_device))
}
