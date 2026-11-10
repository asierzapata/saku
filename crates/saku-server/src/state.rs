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
    kv_generation: String,
}

impl AppState {
    pub fn new(db: Connection, storage: Operator, config: ServerConfig) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                db: Mutex::new(db),
                storage,
                config,
                // A restored database must never restore the identity used by old cursors.
                kv_generation: uuid::Uuid::new_v4().simple().to_string(),
            }),
        }
    }

    pub fn kv_generation(&self) -> &str {
        &self.inner.kv_generation
    }
}
