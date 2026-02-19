use std::sync::{Arc, Mutex};

use opendal::Operator;
use rusqlite::Connection;

use crate::config::ServerConfig;

/// Shared application state, wrapped in `Arc` for axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub db: Mutex<Connection>,
    pub storage: Operator,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new(db: Connection, storage: Operator, config: ServerConfig) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                db: Mutex::new(db),
                storage,
                config,
            }),
        }
    }
}
