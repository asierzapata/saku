use std::path::Path;

use rusqlite::{Connection, params};

use crate::error::SyncError;

/// A record of a tracked file's sync state.
#[derive(Debug, Clone)]
pub struct FileState {
    pub file_key: String,
    pub local_hash: String,
    pub remote_hash: String,
    pub status: String,
    pub updated_at_ms: i64,
}

/// A pending sync operation (upload or delete).
#[derive(Debug, Clone)]
pub struct PendingOp {
    pub id: i64,
    pub file_key: String,
    pub op_type: String,
    pub local_path: String,
    pub created_at_ms: i64,
}

/// Local SQLite database that tracks file states and pending operations.
pub struct StateDb {
    conn: Connection,
}

impl StateDb {
    /// Open (or create) the state database at the given path. Uses WAL mode.
    pub fn open(path: &Path) -> Result<Self, SyncError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self, SyncError> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), SyncError> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_state (
                file_key    TEXT PRIMARY KEY,
                local_hash  TEXT NOT NULL,
                remote_hash TEXT NOT NULL DEFAULT '',
                status      TEXT NOT NULL DEFAULT 'unknown',
                updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pending_ops (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                file_key    TEXT NOT NULL,
                op_type     TEXT NOT NULL,
                local_path  TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );",
        )?;

        Ok(())
    }

    /// Insert or update a file state record.
    pub fn upsert_file_state(&self, state: &FileState) -> Result<(), SyncError> {
        self.conn.execute(
            "INSERT INTO file_state (file_key, local_hash, remote_hash, status, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(file_key) DO UPDATE SET
                local_hash = excluded.local_hash,
                remote_hash = excluded.remote_hash,
                status = excluded.status,
                updated_at_ms = excluded.updated_at_ms",
            params![
                state.file_key,
                state.local_hash,
                state.remote_hash,
                state.status,
                state.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Get a file state by key.
    pub fn get_file_state(&self, file_key: &str) -> Result<Option<FileState>, SyncError> {
        let mut stmt = self.conn.prepare(
            "SELECT file_key, local_hash, remote_hash, status, updated_at_ms
             FROM file_state WHERE file_key = ?1",
        )?;

        let mut rows = stmt.query_map(params![file_key], |row| {
            Ok(FileState {
                file_key: row.get(0)?,
                local_hash: row.get(1)?,
                remote_hash: row.get(2)?,
                status: row.get(3)?,
                updated_at_ms: row.get(4)?,
            })
        })?;

        match rows.next() {
            Some(Ok(state)) => Ok(Some(state)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Get all file state records.
    pub fn all_file_states(&self) -> Result<Vec<FileState>, SyncError> {
        let mut stmt = self.conn.prepare(
            "SELECT file_key, local_hash, remote_hash, status, updated_at_ms FROM file_state",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FileState {
                file_key: row.get(0)?,
                local_hash: row.get(1)?,
                remote_hash: row.get(2)?,
                status: row.get(3)?,
                updated_at_ms: row.get(4)?,
            })
        })?;

        let mut states = Vec::new();
        for row in rows {
            states.push(row?);
        }
        Ok(states)
    }

    /// Enqueue a pending sync operation.
    pub fn enqueue_op(
        &self,
        file_key: &str,
        op_type: &str,
        local_path: &str,
    ) -> Result<(), SyncError> {
        let now_ms = jiff::Timestamp::now().as_millisecond();
        self.conn.execute(
            "INSERT INTO pending_ops (file_key, op_type, local_path, created_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![file_key, op_type, local_path, now_ms],
        )?;
        Ok(())
    }

    /// Get all pending operations, ordered by creation time.
    pub fn pending_ops(&self) -> Result<Vec<PendingOp>, SyncError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_key, op_type, local_path, created_at_ms
             FROM pending_ops ORDER BY created_at_ms ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(PendingOp {
                id: row.get(0)?,
                file_key: row.get(1)?,
                op_type: row.get(2)?,
                local_path: row.get(3)?,
                created_at_ms: row.get(4)?,
            })
        })?;

        let mut ops = Vec::new();
        for row in rows {
            ops.push(row?);
        }
        Ok(ops)
    }

    /// Delete a specific pending operation by id.
    pub fn delete_op(&self, id: i64) -> Result<(), SyncError> {
        self.conn
            .execute("DELETE FROM pending_ops WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Clear all pending operations for a given file key.
    pub fn clear_ops_for_file(&self, file_key: &str) -> Result<(), SyncError> {
        self.conn.execute(
            "DELETE FROM pending_ops WHERE file_key = ?1",
            params![file_key],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crud_file_state() {
        let db = StateDb::open_in_memory().unwrap();

        let state = FileState {
            file_key: "tdo/store.json".to_string(),
            local_hash: "abc".to_string(),
            remote_hash: "".to_string(),
            status: "dirty".to_string(),
            updated_at_ms: 1000,
        };

        db.upsert_file_state(&state).unwrap();

        let fetched = db.get_file_state("tdo/store.json").unwrap().unwrap();
        assert_eq!(fetched.local_hash, "abc");
        assert_eq!(fetched.status, "dirty");

        // Update
        let updated = FileState {
            local_hash: "def".to_string(),
            status: "clean".to_string(),
            ..state.clone()
        };
        db.upsert_file_state(&updated).unwrap();

        let fetched2 = db.get_file_state("tdo/store.json").unwrap().unwrap();
        assert_eq!(fetched2.local_hash, "def");
        assert_eq!(fetched2.status, "clean");
    }

    #[test]
    fn all_file_states() {
        let db = StateDb::open_in_memory().unwrap();

        db.upsert_file_state(&FileState {
            file_key: "a".to_string(),
            local_hash: "h1".to_string(),
            remote_hash: "".to_string(),
            status: "clean".to_string(),
            updated_at_ms: 1,
        })
        .unwrap();

        db.upsert_file_state(&FileState {
            file_key: "b".to_string(),
            local_hash: "h2".to_string(),
            remote_hash: "".to_string(),
            status: "dirty".to_string(),
            updated_at_ms: 2,
        })
        .unwrap();

        let all = db.all_file_states().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn pending_ops_lifecycle() {
        let db = StateDb::open_in_memory().unwrap();

        db.enqueue_op("tdo/store.json", "upload", "/path/to/store.json")
            .unwrap();
        db.enqueue_op("tdo/store.json", "upload", "/path/to/store.json")
            .unwrap();

        let ops = db.pending_ops().unwrap();
        assert_eq!(ops.len(), 2);

        // Delete one
        db.delete_op(ops[0].id).unwrap();
        assert_eq!(db.pending_ops().unwrap().len(), 1);

        // Clear all for file
        db.enqueue_op("tdo/store.json", "upload", "/path/to/store.json")
            .unwrap();
        db.clear_ops_for_file("tdo/store.json").unwrap();
        assert_eq!(db.pending_ops().unwrap().len(), 0);
    }

    #[test]
    fn get_missing_file_state() {
        let db = StateDb::open_in_memory().unwrap();
        let result = db.get_file_state("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn idempotent_schema_creation() {
        let db = StateDb::open_in_memory().unwrap();
        // init_schema was called in open_in_memory; calling it again should be fine
        db.init_schema().unwrap();
    }
}
