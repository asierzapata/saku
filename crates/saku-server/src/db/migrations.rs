use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // Migration 1: initial schema
    "
    CREATE TABLE IF NOT EXISTS users (
        id TEXT PRIMARY KEY,
        email TEXT NOT NULL UNIQUE,
        password_hash TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        quota_bytes INTEGER NOT NULL DEFAULT 104857600
    );

    CREATE TABLE IF NOT EXISTS devices (
        id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        device_name TEXT NOT NULL DEFAULT '',
        last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS refresh_tokens (
        token_hash TEXT PRIMARY KEY,
        device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
        user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        expires_at TEXT NOT NULL,
        revoked INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS storage_quota (
        user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        used_bytes INTEGER NOT NULL DEFAULT 0,
        last_updated TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS schema_version (
        version INTEGER NOT NULL
    );

    INSERT INTO schema_version (version) VALUES (1);
    ",
    // Migration 2: per-entry KV sync tables
    "
    CREATE TABLE IF NOT EXISTS kv_entries (
        user_id     TEXT NOT NULL,
        tool        TEXT NOT NULL,
        key         TEXT NOT NULL,
        blob        BLOB NOT NULL,
        seq         INTEGER NOT NULL,
        deleted     BOOLEAN DEFAULT FALSE,
        written_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
        PRIMARY KEY (user_id, tool, key)
    );

    CREATE INDEX IF NOT EXISTS idx_kv_seq ON kv_entries (user_id, tool, seq);

    CREATE TABLE IF NOT EXISTS kv_seq_counters (
        user_id     TEXT NOT NULL,
        tool        TEXT NOT NULL,
        next_seq    INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (user_id, tool)
    );

    UPDATE schema_version SET version = 2;
    ",
];

/// Run all pending migrations.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let migration_version = (i + 1) as i64;
        if migration_version > current_version {
            conn.execute_batch(sql)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_on_fresh_db() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='users'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // Should not error
    }
}
