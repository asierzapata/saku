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
#[allow(dead_code)]
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

/// Batch upsert entries within a single transaction.
/// Returns (results, cookie) where cookie is the string representation of the max seq after upsert.
pub fn batch_upsert(
    conn: &Connection,
    user_id: &str,
    tool: &str,
    entries: &[(String, Vec<u8>)],
) -> Result<(Vec<UpsertResult>, String), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;

    // Ensure seq counter exists
    tx.execute(
        "INSERT OR IGNORE INTO kv_seq_counters (user_id, tool, next_seq) VALUES (?1, ?2, 1)",
        params![user_id, tool],
    )?;

    let mut next_seq: i64 = tx.query_row(
        "SELECT next_seq FROM kv_seq_counters WHERE user_id = ?1 AND tool = ?2",
        params![user_id, tool],
        |row| row.get(0),
    )?;

    let mut results = Vec::with_capacity(entries.len());

    for (key, blob) in entries {
        let seq = next_seq;
        tx.execute(
            "INSERT INTO kv_entries (user_id, tool, key, blob, seq)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id, tool, key) DO UPDATE SET
                blob = excluded.blob,
                seq = excluded.seq,
                written_at = strftime('%s','now')",
            params![user_id, tool, key, blob, seq],
        )?;
        results.push(UpsertResult {
            key: key.clone(),
            seq,
        });
        next_seq += 1;
    }

    tx.execute(
        "UPDATE kv_seq_counters SET next_seq = ?3 WHERE user_id = ?1 AND tool = ?2",
        params![user_id, tool, next_seq],
    )?;

    tx.commit()?;

    let cookie = (next_seq - 1).to_string();
    Ok((results, cookie))
}

/// Upsert a single entry. Returns the assigned seq number.
pub fn upsert_single(
    conn: &Connection,
    user_id: &str,
    tool: &str,
    key: &str,
    blob: &[u8],
) -> Result<i64, rusqlite::Error> {
    let (results, _) = batch_upsert(conn, user_id, tool, &[(key.to_string(), blob.to_vec())])?;
    Ok(results[0].seq)
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
        let (results, _) = batch_upsert(&conn, "user1", "tdo", &entries).unwrap();
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
        batch_upsert(&conn, "user1", "tdo", &batch1).unwrap();

        let batch2: Vec<(String, Vec<u8>)> = vec![
            ("d".into(), b"4".to_vec()),
            ("e".into(), b"5".to_vec()),
            ("f".into(), b"6".to_vec()),
        ];
        let (results, _) = batch_upsert(&conn, "user1", "tdo", &batch2).unwrap();
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
            &[("key1".into(), b"old".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user1",
            "tdo",
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
        batch_upsert(&conn, "user1", "tdo", &entries).unwrap();

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
        batch_upsert(&conn, "user1", "tdo", &entries).unwrap();

        let result = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn get_entries_since_respects_limit() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = (1..=5)
            .map(|i| (format!("key{i}"), format!("blob{i}").into_bytes()))
            .collect();
        batch_upsert(&conn, "user1", "tdo", &entries).unwrap();

        let result = get_entries_since(&conn, "user1", "tdo", 0, 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].seq, 1);
        assert_eq!(result[1].seq, 2);
    }

    #[test]
    fn get_entries_since_empty_when_up_to_date() {
        let conn = setup_db();
        let entries: Vec<(String, Vec<u8>)> = vec![("a".into(), b"1".to_vec())];
        batch_upsert(&conn, "user1", "tdo", &entries).unwrap();

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
        let (_, cookie) = batch_upsert(&conn, "user1", "tdo", &entries).unwrap();
        assert_eq!(cookie, "3");
    }

    #[test]
    fn upsert_single_works() {
        let conn = setup_db();
        let seq = upsert_single(&conn, "user1", "tdo", "mykey", b"myblob").unwrap();
        assert_eq!(seq, 1);

        let entries = get_entries_since(&conn, "user1", "tdo", 0, 100).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "mykey");
        assert_eq!(entries[0].blob, b"myblob");
    }

    #[test]
    fn different_tools_have_independent_seqs() {
        let conn = setup_db();
        batch_upsert(
            &conn,
            "user1",
            "tdo",
            &[("a".into(), b"1".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user1",
            "nte",
            &[("b".into(), b"2".to_vec())],
        )
        .unwrap();

        // nte should start at seq 1, not continue from tdo's seq
        assert_eq!(results[0].seq, 1);
    }

    #[test]
    fn different_users_have_independent_seqs() {
        let conn = setup_db();
        batch_upsert(
            &conn,
            "user1",
            "tdo",
            &[("a".into(), b"1".to_vec()), ("b".into(), b"2".to_vec())],
        )
        .unwrap();

        let (results, _) = batch_upsert(
            &conn,
            "user2",
            "tdo",
            &[("c".into(), b"3".to_vec())],
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
