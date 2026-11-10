use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
pub struct KvEntry {
    pub key: String,
    pub blob: Vec<u8>,
    pub seq: i64,
    pub deleted: bool,
}

#[derive(Debug, Clone)]
pub struct UpsertResult {
    pub key: String,
    pub seq: i64,
}

/// Fetch entries with seq > after_seq, ordered by seq, limited to `limit` rows.
pub fn get_entries_since(
    conn: &Connection,
    user_id: &str,
    tool: &str,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<KvEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT key, blob, seq, deleted FROM kv_entries
         WHERE user_id = ?1 AND tool = ?2 AND seq > ?3
         ORDER BY seq ASC
         LIMIT ?4",
    )?;

    let entries = stmt
        .query_map(params![user_id, tool, after_seq, limit], |row| {
            Ok(KvEntry {
                key: row.get(0)?,
                blob: row.get(1)?,
                seq: row.get(2)?,
                deleted: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Get the current maximum seq for a (user_id, tool) pair. Returns 0 if no entries exist.
pub fn get_max_seq(
    conn: &Connection,
    user_id: &str,
    tool: &str,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM kv_entries WHERE user_id = ?1 AND tool = ?2",
        params![user_id, tool],
        |row| row.get(0),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum KvWriteError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    InvalidRequest(String),
    #[error("entries changed since the last pull")]
    StaleWrite,
    #[error("server sequence exhausted")]
    SequenceExhausted,
}

pub fn batch_upsert(
    conn: &Connection,
    user_id: &str,
    tool: &str,
    base_seq: i64,
    entries: &[(String, Vec<u8>)],
) -> Result<(Vec<UpsertResult>, String), KvWriteError> {
    use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};
    let mut keys = std::collections::HashSet::new();
    if base_seq < 0 || entries.is_empty() || entries.iter().any(|(key, _)| !keys.insert(key)) {
        return Err(KvWriteError::InvalidRequest("require a nonnegative base cookie and a nonempty batch of unique keys".into()));
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let next_seq: i64 = tx.query_row(
        "SELECT next_seq FROM kv_seq_counters WHERE user_id = ?1 AND tool = ?2",
        params![user_id, tool],
        |row| row.get(0),
    ).optional()?.unwrap_or(1);
    let current_seq = next_seq.checked_sub(1).filter(|seq| *seq >= 0).ok_or(KvWriteError::SequenceExhausted)?;
    if base_seq > current_seq {
        return Err(KvWriteError::InvalidRequest("base cookie is ahead of server state; recovery required".into()));
    }
    for (key, _) in entries {
        let seq: Option<i64> = tx.query_row(
            "SELECT seq FROM kv_entries WHERE user_id = ?1 AND tool = ?2 AND key = ?3",
            params![user_id, tool, key], |row| row.get(0),
        ).optional()?;
        if seq.is_some_and(|seq| seq > base_seq) {
            return Err(KvWriteError::StaleWrite);
        }
    }
    let count = i64::try_from(entries.len()).map_err(|_| KvWriteError::SequenceExhausted)?;
    let after_seq = next_seq.checked_add(count).ok_or(KvWriteError::SequenceExhausted)?;
    let mut results = Vec::with_capacity(entries.len());
    for (index, (key, blob)) in entries.iter().enumerate() {
        let seq = next_seq + index as i64;
        tx.execute(
            "INSERT INTO kv_entries (user_id, tool, key, blob, seq) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id, tool, key) DO UPDATE SET blob = excluded.blob, seq = excluded.seq, written_at = strftime('%s','now')",
            params![user_id, tool, key, blob, seq],
        )?;
        results.push(UpsertResult { key: key.clone(), seq });
    }
    tx.execute(
        "INSERT INTO kv_seq_counters (user_id, tool, next_seq) VALUES (?1, ?2, ?3)
         ON CONFLICT(user_id, tool) DO UPDATE SET next_seq = excluded.next_seq",
        params![user_id, tool, after_seq],
    )?;
    tx.commit()?;
    Ok((results, (after_seq - 1).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn batch_upsert_assigns_sequential_seqs() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = vec![
            ("key1".into(), b"blob1".to_vec()),
            ("key2".into(), b"blob2".to_vec()),
            ("key3".into(), b"blob3".to_vec()),
        ];
        let (results, _) = batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].seq, 1);
        assert_eq!(results[1].seq, 2);
        assert_eq!(results[2].seq, 3);
    }

    #[test]
    fn batch_upsert_second_batch_continues_sequence() {
        let conn = setup_db();
        let batch1: Vec<(String, Vec<u8>)> = vec![
            ("a".into(), b"1".to_vec()),
            ("b".into(), b"2".to_vec()),
            ("c".into(), b"3".to_vec()),
        ];
        batch_upsert(&conn, "user1", "tdo", 0, &batch1).unwrap();

        let batch2: Vec<(String, Vec<u8>)> = vec![
            ("d".into(), b"4".to_vec()),
            ("e".into(), b"5".to_vec()),
            ("f".into(), b"6".to_vec()),
        ];
        let (results, _) = batch_upsert(&conn, "user1", "tdo", 0, &batch2).unwrap();
        assert_eq!(results[0].seq, 4);
        assert_eq!(results[1].seq, 5);
        assert_eq!(results[2].seq, 6);
    }

    #[test]
    fn batch_upsert_updates_existing_key() {
        let conn = setup_db();
        batch_upsert(
            &conn,
            "user1",
            "tdo",
            0,
            &[("key1".into(), b"old".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user1",
            "tdo",
            1,
            &[("key1".into(), b"new".to_vec())],
        )
        .unwrap();

        // key1 should get a new seq
        assert_eq!(results[0].seq, 2);

        // verify the blob was updated
        let entries = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].blob, b"new");
        assert_eq!(entries[0].seq, 2);
    }

    #[test]
    fn get_entries_since_returns_only_new() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = (1..=5)
            .map(|i| (format!("key{i}"), format!("blob{i}").into_bytes()))
            .collect();
        batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();

        let result = get_entries_since(&conn, "user1", "tdo", 3, 100).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].seq, 4);
        assert_eq!(result[1].seq, 5);
    }

    #[test]
    fn get_entries_since_zero_returns_all() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = vec![
            ("a".into(), b"1".to_vec()),
            ("b".into(), b"2".to_vec()),
        ];
        batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();

        let result = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn get_entries_since_respects_limit() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = (1..=5)
            .map(|i| (format!("key{i}"), format!("blob{i}").into_bytes()))
            .collect();
        batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();

        let result = get_entries_since(&conn, "user1", "tdo", 0, 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].seq, 1);
        assert_eq!(result[1].seq, 2);
    }

    #[test]
    fn get_entries_since_empty_when_up_to_date() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = vec![("a".into(), b"1".to_vec())];
        batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();

        let max = get_max_seq(&conn, "user1", "tdo").unwrap();
        let result = get_entries_since(&conn, "user1", "tdo", max, 100).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn batch_upsert_returns_correct_cookie() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = vec![
            ("a".into(), b"1".to_vec()),
            ("b".into(), b"2".to_vec()),
            ("c".into(), b"3".to_vec()),
        ];
        let (_, cookie) = batch_upsert(&conn, "user1", "tdo", 0, &entries).unwrap();
        assert_eq!(cookie, "3");
    }

    #[test]
    fn stale_batch_is_atomic_and_validation_does_not_consume_sequences() {
        let conn = setup_db();
        let old = vec![("mykey".into(), b"old".to_vec())];
        batch_upsert(&conn, "user1", "tdo", 0, &old).unwrap();
        let mixed = vec![("new".into(), b"new".to_vec()), ("mykey".into(), b"stale".to_vec())];
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", 0, &mixed), Err(KvWriteError::StaleWrite)));
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", -1, &old), Err(KvWriteError::InvalidRequest(_))));
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", 2, &old), Err(KvWriteError::InvalidRequest(_))));
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", 1, &[]), Err(KvWriteError::InvalidRequest(_))));
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", 1, &[old[0].clone(), old[0].clone()]), Err(KvWriteError::InvalidRequest(_))));
        let entries = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "mykey");
        assert_eq!(entries[0].blob, b"old");
        assert_eq!(batch_upsert(&conn, "user1", "tdo", 1, &mixed).unwrap().1, "3");
    }

    #[test]
    fn counter_write_failure_and_sequence_exhaustion_roll_back() {
        let conn = setup_db();
        let entry = vec![("a".into(), b"old".to_vec())];
        batch_upsert(&conn, "user1", "tdo", 0, &entry).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_counter BEFORE UPDATE ON kv_seq_counters BEGIN SELECT RAISE(ABORT, 'counter failed'); END;").unwrap();
        let changed = vec![("a".into(), b"new".to_vec()), ("b".into(), b"new".to_vec())];
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", 1, &changed), Err(KvWriteError::Db(_))));
        let entries = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].blob, b"old");
        assert_eq!(entries[0].seq, 1);
        assert_eq!(conn.query_row("SELECT next_seq FROM kv_seq_counters", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
        conn.execute_batch("DROP TRIGGER fail_counter").unwrap();
        conn.execute("UPDATE kv_seq_counters SET next_seq = ?1", [i64::MAX]).unwrap();
        assert!(matches!(batch_upsert(&conn, "user1", "tdo", i64::MAX - 1, &changed), Err(KvWriteError::SequenceExhausted)));
        assert_eq!(get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap()[0].blob, b"old");
    }

    #[test]
    fn different_tools_have_independent_seqs() {
        let conn = setup_db();
        batch_upsert(
            &conn,
            "user1",
            "tdo",
            0,
            &[("a".into(), b"1".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user1",
            "other-tool",
            0,
            &[("a".into(), b"2".to_vec())],
        )
        .unwrap();

        // other-tool should start at seq 1, not continue from tdo's seq
        assert_eq!(results[0].seq, 1);
    }

    #[test]
    fn different_users_have_independent_seqs() {
        let conn = setup_db();
        batch_upsert(
            &conn,
            "user1",
            "tdo",
            0,
            &[("a".into(), b"1".to_vec()), ("b".into(), b"2".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user2",
            "tdo",
            0,
            &[("a".into(), b"3".to_vec())],
        )
        .unwrap();

        assert_eq!(results[0].seq, 1);
    }

    #[test]
    fn migration_2_creates_kv_tables() {
        let conn = setup_db();

        // Verify kv_entries table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_entries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify kv_seq_counters table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_seq_counters'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify index exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_kv_seq'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
