use saku_sync::replica::Replica;
use saku_tdo::models::task::Task;
use saku_tdo::storage::sqlite::SqliteStorage;
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Output};

fn task(title: &str, wall_ms: i64) -> Value {
    let mut task = Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: title.into(),
        notes: Some("preserve this note".into()),
        ..Task::default()
    };
    task.modified_at.wall_ms = wall_ms;
    let mut value = serde_json::to_value(task).unwrap();
    value["created_at"] = json!("2026-01-01T00:00:00Z");
    value
}

fn import(storage: &mut SqliteStorage, entries: Value) {
    storage
        .import_json(&json!({"version":9,"entries":entries}).to_string())
        .unwrap();
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
}

fn state(
    storage: &SqliteStorage,
    path: &Path,
) -> (
    Value,
    Vec<saku_tdo::storage::sqlite::PendingChange>,
    String,
    i64,
    Option<String>,
    String,
) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let (clock, generation) = connection
        .query_row(
            "SELECT clock, generation FROM store_metadata WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let cookie = connection
        .query_row("SELECT cookie FROM sync_state WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    (
        serde_json::from_str(&storage.export_json().unwrap()).unwrap(),
        storage.pending_changes().unwrap(),
        clock,
        generation,
        cookie,
        storage.export_history_json(None).unwrap(),
    )
}

fn cli(dir: &tempfile::TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tdo"))
        .env("TDO_DATA_DIR", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env_remove("TDO_NO_SYNC")
        .env("TDO_SYNC_DIR", dir.path().join("sentinel"))
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn divergent_pull_preserves_local_and_remote_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.sqlite");
    let local = task("local", 100);
    let remote = task("remote", 200);
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(&json!({"version":9,"entries":{"task/a":local}}).to_string())
        .unwrap();
    storage.prepare_sync("server").unwrap();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(exported["entries"]["task/a"], remote);
    assert!(storage.pending_changes().unwrap().is_empty());
    let connection = rusqlite::Connection::open(&path).unwrap();
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='recovery_history'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        exists, 1,
        "divergent pull must preserve both full JSON values"
    );
    let mut statement = connection
        .prepare("SELECT value FROM recovery_history WHERE key = 'task/a' ORDER BY id")
        .unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    let values: Vec<Value> = rows
        .map(|row| serde_json::from_str(&row.unwrap()).unwrap())
        .collect();
    assert_eq!(values, vec![local, remote]);
}

#[test]
fn both_winner_directions_keep_full_values_and_replay_is_idempotent() {
    for (local_ms, remote_ms, winner) in [
        (100, 200, "remote"),
        (200, 100, "local"),
        (100, 100, "remote"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let mut local = task("local", local_ms);
        if local_ms != remote_ms {
            local["estimate_minutes"] = Value::Null;
        }
        let remote = task("remote", remote_ms);
        let mut storage = SqliteStorage::open(&path).unwrap();
        import(&mut storage, json!({"task/a":local}));
        storage
            .apply_pull(
                "server",
                None,
                "1",
                &[("task/a".into(), remote.to_string())],
            )
            .unwrap();
        let current: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        assert_eq!(current["entries"]["task/a"]["title"], winner);
        let history = storage.history_entries(Some("task/a")).unwrap();
        assert_eq!(history.len(), 2);
        assert!(
            history
                .iter()
                .any(|entry| entry.value == local && entry.source == "local")
        );
        assert!(
            history
                .iter()
                .any(|entry| entry.value == remote && entry.source == "remote")
        );
        if winner == "local" {
            assert_eq!(
                storage.pending_changes().unwrap()[0].value.as_ref(),
                Some(&current["entries"]["task/a"])
            );
        } else if current["entries"]["task/a"] == remote {
            assert!(storage.pending_changes().unwrap().is_empty());
        }
        let previous = storage.export_history_json(None).unwrap();
        let reversed = format!(
            "{{{}}}",
            remote
                .as_object()
                .unwrap()
                .iter()
                .rev()
                .map(|(key, value)| format!("{}:{}", serde_json::to_string(key).unwrap(), value))
                .collect::<Vec<_>>()
                .join(",")
        );
        storage
            .apply_pull("server", Some("1"), "2", &[("task/a".into(), reversed)])
            .unwrap();
        assert_eq!(storage.export_history_json(None).unwrap(), previous);
        drop(storage);
        let reopened = SqliteStorage::open(&path).unwrap();
        assert_eq!(reopened.export_history_json(None).unwrap(), previous);
        assert_eq!(reopened.history_entries(Some("task/a")).unwrap(), history);
    }
}

#[test]
fn identical_values_and_normal_new_entries_are_not_archived() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let local = task("same", 100);
    import(&mut storage, json!({"task/a":local}));
    let mut second = task("new", 150);
    second["storage_key_suffix"] = json!("b");
    second["task_number"] = json!(2);
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("task/a".into(), local.to_string()),
                ("task/b".into(), second.to_string()),
            ],
        )
        .unwrap();
    assert!(storage.history_entries(None).unwrap().is_empty());
}

#[test]
fn provenance_is_kept_when_a_version_reappears_on_the_other_side() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let local = task("local", 100);
    let remote = task("remote", 200);
    import(&mut storage, json!({"task/a":local}));
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let before = storage.export_history_json(None).unwrap();
    storage
        .apply_pull(
            "server",
            Some("1"),
            "2",
            &[("task/a".into(), local.to_string())],
        )
        .unwrap();
    assert_eq!(storage.export_history_json(None).unwrap(), before);
    let history = storage.history_entries(None).unwrap();
    assert_eq!(
        (history[0].source.as_str(), &history[0].value),
        ("local", &local)
    );
    assert_eq!(
        (history[1].source.as_str(), &history[1].value),
        ("remote", &remote)
    );
    assert_eq!(
        storage.pending_changes().unwrap()[0].value.as_ref(),
        Some(&remote)
    );
}

#[test]
fn exact_key_filter_handles_sql_wildcards() {
    use saku_tdo::models::project::Project;
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut percent = serde_json::to_value(Project {
        name: "a%".into(),
        ..Project::default()
    })
    .unwrap();
    let mut underscore = serde_json::to_value(Project {
        name: "a_".into(),
        ..Project::default()
    })
    .unwrap();
    percent["modified_at"]["wall_ms"] = json!(100);
    underscore["modified_at"]["wall_ms"] = json!(100);
    import(
        &mut storage,
        json!({"project/a%":percent,"project/a_":underscore}),
    );
    let mut a = percent.clone();
    let mut b = underscore.clone();
    a["modified_at"]["wall_ms"] = json!(200);
    b["modified_at"]["wall_ms"] = json!(200);
    a["notes"] = json!("new %");
    b["notes"] = json!("new _");
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("project/a%".into(), a.to_string()),
                ("project/a_".into(), b.to_string()),
            ],
        )
        .unwrap();
    assert_eq!(storage.history_entries(None).unwrap().len(), 4);
    for key in ["project/a%", "project/a_"] {
        let entries = storage.history_entries(Some(key)).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.key == key));
    }
    assert!(
        storage
            .history_entries(Some("project/a"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn tombstone_and_reference_repairs_preserve_original_and_result() {
    use saku_tdo::models::area::Area;
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut original = task("important", 10);
    original["area_key"] = json!("area/old");
    original["depends_on"] = json!(["task/b"]);
    let mut dependency = task("dependency", 10);
    dependency["storage_key_suffix"] = json!("b");
    dependency["task_number"] = json!(2);
    let mut old = serde_json::to_value(Area {
        name: "old".into(),
        ..Area::default()
    })
    .unwrap();
    old["modified_at"]["wall_ms"] = json!(10);
    import(
        &mut storage,
        json!({"task/a":original,"task/b":dependency,"area/old":old}),
    );
    let mut tombstone = old.clone();
    tombstone["modified_at"]["wall_ms"] = json!(20);
    tombstone["deleted_at"] = json!("2026-01-01T00:00:00Z");
    tombstone["renamed_to"] = json!("area/new");
    let mut replacement = serde_json::to_value(Area {
        name: "new".into(),
        ..Area::default()
    })
    .unwrap();
    replacement["previous_key"] = json!("area/old");
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("area/old".into(), tombstone.to_string()),
                ("area/new".into(), replacement.to_string()),
            ],
        )
        .unwrap();
    let history = storage.history_entries(Some("area/old")).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].value, old);
    assert_eq!(history[1].value, tombstone);
    let history = storage.history_entries(Some("task/a")).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        (history[0].source.as_str(), &history[0].value),
        ("local", &original)
    );
    let mut repaired = original.clone();
    repaired["area_key"] = json!("area/new");
    assert_eq!(
        (history[1].source.as_str(), &history[1].value),
        ("merged", &repaired)
    );
}

#[test]
fn local_only_number_repair_preserves_original_and_result() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut local = task("local", 100);
    local["storage_key_suffix"] = json!("z");
    let remote = task("remote", 200);
    import(&mut storage, json!({"task/z":local}));
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let history = storage.history_entries(None).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].key, "task/z");
    assert_eq!(
        (history[0].source.as_str(), &history[0].value),
        ("local", &local)
    );
    let mut repaired = local.clone();
    repaired["task_number"] = json!(2);
    assert_eq!(
        (history[1].source.as_str(), &history[1].value),
        ("merged", &repaired)
    );
    assert_eq!(history[1].key, "task/z");
    assert!(storage.history_entries(Some("task/a")).unwrap().is_empty());
    assert_eq!(
        storage.pending_changes().unwrap()[0].value.as_ref(),
        Some(&repaired)
    );
}

#[test]
fn remote_legacy_schedule_archives_raw_and_normalized_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    storage.prepare_sync("server").unwrap();
    let mut remote = task("today", 100);
    remote["when"] = json!({"type":"Today","evening":false});
    let before = jiff::Zoned::now().date();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let after = jiff::Zoned::now().date();
    let history = storage.history_entries(Some("task/a")).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        (history[0].source.as_str(), &history[0].value),
        ("remote", &remote)
    );
    assert_eq!(history[1].source, "merged");
    assert_eq!(history[1].value["when"]["type"], "Scheduled");
    let date = history[1].value["when"]["date"]
        .as_str()
        .unwrap()
        .parse::<jiff::civil::Date>()
        .unwrap();
    assert!(date == before || date == after);
    let mut expected = remote.clone();
    expected["when"] = json!({"type":"Scheduled","date":date.to_string()});
    assert_eq!(history[1].value, expected);
}

#[test]
fn three_distinct_merge_versions_preserve_both_inputs_and_result() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut local = task("local", 100);
    local["when"] = json!({"type":"Scheduled","date":"2020-01-01"});
    let mut remote = task("remote", 200);
    remote["when"] = json!({"type":"Today","evening":false});
    import(&mut storage, json!({"task/a":local}));
    let before = jiff::Zoned::now().date();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let after = jiff::Zoned::now().date();
    let history = storage.history_entries(Some("task/a")).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        (history[0].source.as_str(), &history[0].value),
        ("local", &local)
    );
    assert_eq!(
        (history[1].source.as_str(), &history[1].value),
        ("remote", &remote)
    );
    assert_eq!(history[2].source, "merged");
    let date = history[2].value["when"]["date"]
        .as_str()
        .unwrap()
        .parse::<jiff::civil::Date>()
        .unwrap();
    assert!(date == before || date == after);
    let mut expected = remote.clone();
    expected["when"] = json!({"type":"Scheduled","date":date.to_string()});
    assert_eq!(history[2].value, expected);
    assert_eq!(
        storage.pending_changes().unwrap()[0].value.as_ref(),
        Some(&expected)
    );
}

#[test]
fn insertion_and_late_cursor_failures_roll_back_all_state() {
    for (trigger, table) in [
        ("fail_history", "recovery_history"),
        ("fail_cursor", "sync_state"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut storage = SqliteStorage::open(&path).unwrap();
        let local = task("local", 100);
        let remote = task("remote", 200);
        import(&mut storage, json!({"task/a":local}));
        let before = state(&storage, &path);
        let connection = rusqlite::Connection::open(&path).unwrap();
        let operation = if table == "recovery_history" {
            "INSERT"
        } else {
            "UPDATE"
        };
        connection.execute_batch(&format!("CREATE TRIGGER {trigger} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT, 'injected failure'); END;")).unwrap();
        assert!(
            storage
                .apply_pull(
                    "server",
                    None,
                    "1",
                    &[("task/a".into(), remote.to_string())]
                )
                .is_err()
        );
        assert_eq!(state(&storage, &path), before);
        drop(storage);
        let mut storage = SqliteStorage::open(&path).unwrap();
        assert_eq!(state(&storage, &path), before);
        connection
            .execute_batch(&format!("DROP TRIGGER {trigger};"))
            .unwrap();
        storage
            .apply_pull(
                "server",
                None,
                "1",
                &[("task/a".into(), remote.to_string())],
            )
            .unwrap();
        let after = state(&storage, &path);
        assert_ne!(after.0, before.0);
        assert!(after.1.is_empty());
        assert_ne!(after.2, before.2);
        assert!(after.3 > before.3);
        assert_eq!(after.4.as_deref(), Some("1"));
        assert_eq!(storage.history_entries(None).unwrap().len(), 2);
    }
}

#[test]
fn version_three_upgrade_preserves_state_and_rolls_back_on_existing_table() {
    for bad in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut storage = SqliteStorage::open(&path).unwrap();
        let local = task("local", 100);
        import(&mut storage, json!({"task/a":local}));
        storage.apply_pull("server", None, "1", &[]).unwrap();
        let before = state(&storage, &path);
        drop(storage);
        let connection = rusqlite::Connection::open(&path).unwrap();
        if !bad {
            connection
                .execute_batch("DROP TABLE recovery_history;")
                .unwrap();
        }
        connection
            .execute_batch("PRAGMA user_version = 3;")
            .unwrap();
        drop(connection);
        if bad {
            assert!(SqliteStorage::open(&path).is_err());
            let connection = rusqlite::Connection::open(&path).unwrap();
            let version: i64 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap();
            assert_eq!(version, 3);
            let metadata: (String, i64) = connection
                .query_row("SELECT clock, generation FROM store_metadata", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .unwrap();
            assert_eq!(metadata, (before.2, before.3));
            let raw: String = connection
                .query_row(
                    "SELECT value FROM entries WHERE key = 'task/a'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), local);
            let pending: (i64, String) = connection
                .query_row(
                    "SELECT revision, value FROM pending_changes WHERE key = 'task/a'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(pending.0, before.1[0].revision);
            assert_eq!(
                serde_json::from_str::<Value>(&pending.1).unwrap(),
                *before.1[0].value.as_ref().unwrap()
            );
            let cookie: Option<String> = connection
                .query_row("SELECT cookie FROM sync_state WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(cookie, before.4);
        } else {
            let storage = SqliteStorage::open(&path).unwrap();
            assert_eq!(state(&storage, &path), before);
            assert!(storage.history_entries(None).unwrap().is_empty());
            drop(storage);
            let storage = SqliteStorage::open(&path).unwrap();
            assert_eq!(state(&storage, &path), before);
            let connection = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                connection
                    .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                    .unwrap(),
                4
            );
        }
    }
}

#[test]
fn history_only_database_is_not_pristine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut storage = SqliteStorage::open(&path).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute("INSERT INTO recovery_history (key,value,source,recorded_at_ms) VALUES ('task/a',?1,'remote',1)", [task("history", 100).to_string()]).unwrap();
    assert!(!storage.is_pristine().unwrap());
    assert!(!SqliteStorage::existing_is_pristine(&path).unwrap());
    assert!(
        storage
            .import_json(r#"{"version":9,"entries":{}}"#)
            .is_err()
    );
}

#[test]
fn version_three_pristine_guard_is_read_only_and_validates_clock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    drop(SqliteStorage::open(&path).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE recovery_history; PRAGMA user_version = 3;")
        .unwrap();
    assert!(SqliteStorage::existing_is_pristine(&path).unwrap());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
    connection
        .execute(
            "UPDATE store_metadata SET clock = ?1",
            [r#"{"wall_ms":1,"wall_ms":2}"#],
        )
        .unwrap();
    assert!(SqliteStorage::existing_is_pristine(&path).is_err());
    assert!(SqliteStorage::open(&path).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'recovery_history'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn version_three_upgrade_rejects_corrupt_original_rows() {
    for corrupt in ["entry", "pending", "binding", "clock"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        drop(SqliteStorage::open(&path).unwrap());
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE recovery_history; PRAGMA user_version = 3;")
            .unwrap();
        match corrupt {
            "entry" => connection
                .execute("INSERT INTO entries (key,value) VALUES ('task/a','{}')", [])
                .unwrap(),
            "pending" => connection
                .execute(
                    "INSERT INTO pending_changes (key,revision,value) VALUES ('task/a',1,'{}')",
                    [],
                )
                .unwrap(),
            "binding" => connection
                .execute(
                    "INSERT INTO sync_state (id,target,cookie) VALUES (1,X'FF',NULL)",
                    [],
                )
                .unwrap(),
            "clock" => connection
                .execute("UPDATE store_metadata SET clock = 'invalid'", [])
                .unwrap(),
            _ => unreachable!(),
        };
        assert!(SqliteStorage::open(&path).is_err(), "{corrupt}");
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            3,
            "{corrupt}"
        );
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'recovery_history'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{corrupt}");
    }
}

#[test]
fn malformed_archived_values_fail_on_list_show_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let storage = SqliteStorage::open(&path).unwrap();
    let connection = rusqlite::Connection::open(path).unwrap();
    let invalid = r#"{"modified_at":{"wall_ms":1,"wall_ms":2}}"#;
    connection.execute("INSERT INTO recovery_history (key,value,source,recorded_at_ms) VALUES ('task/a',?1,'remote',1)", [invalid]).unwrap();
    assert!(storage.history_entries(None).is_err());
    assert!(storage.history_entry(1).is_err());
    assert!(storage.export_history_json(None).is_err());
}

#[test]
fn cli_history_is_local_read_only_and_export_is_no_clobber() {
    use saku_tdo::models::area::Area;
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let sentinel = dir.path().join("sentinel");
    fs::create_dir(&sentinel).unwrap();
    fs::write(sentinel.join("keep"), b"untouched").unwrap();
    fs::create_dir(dir.path().join("saku")).unwrap();
    let config = dir.path().join("saku/sync.toml");
    fs::write(&config, "not valid toml [[[ ").unwrap();
    let empty: Value =
        serde_json::from_slice(&success(cli(&dir, &["history", "list", "--json"])).stdout).unwrap();
    assert_eq!(empty, json!([]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let local = task("local", 100);
    let remote = task("remote", 200);
    let mut area = serde_json::to_value(Area {
        name: "work".into(),
        ..Area::default()
    })
    .unwrap();
    area["modified_at"]["wall_ms"] = json!(100);
    let mut remote_area = area.clone();
    remote_area["modified_at"]["wall_ms"] = json!(200);
    import(&mut storage, json!({"task/a":local,"area/work":area}));
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("task/a".into(), remote.to_string()),
                ("area/work".into(), remote_area.to_string()),
            ],
        )
        .unwrap();
    let before = state(&storage, &path);
    let expected = storage.history_entries(None).unwrap();
    let task_history = storage.history_entries(Some("task/a")).unwrap();
    let id = task_history[0].id.to_string();
    let listed: Value =
        serde_json::from_slice(&success(cli(&dir, &["history", "list", "--json"])).stdout).unwrap();
    assert_eq!(listed, serde_json::to_value(&expected).unwrap());
    let filtered: Value = serde_json::from_slice(
        &success(cli(&dir, &["history", "list", "--json", "--key", "task/a"])).stdout,
    )
    .unwrap();
    assert_eq!(filtered, serde_json::to_value(&task_history).unwrap());
    let absent: Value = serde_json::from_slice(
        &success(cli(&dir, &["history", "list", "--json", "--key", "task/"])).stdout,
    )
    .unwrap();
    assert_eq!(absent, json!([]));
    let table = success(cli(&dir, &["history", "list"]));
    let table = String::from_utf8(table.stdout).unwrap();
    assert!(table.starts_with("ID\tKEY\tSOURCE\tRECORDED_AT_MS\n"));
    assert!(!table.contains("preserve this note"));
    let shown: Value =
        serde_json::from_slice(&success(cli(&dir, &["history", "show", &id])).stdout).unwrap();
    assert_eq!(shown["value"]["notes"], "preserve this note");
    assert_eq!(shown, serde_json::to_value(&task_history[0]).unwrap());
    assert!(!cli(&dir, &["history", "show", "999999"]).status.success());
    assert!(!cli(&dir, &["history", "show", "0"]).status.success());
    assert!(!cli(&dir, &["history", "show", "abc"]).status.success());
    let stdout: Value =
        serde_json::from_slice(&success(cli(&dir, &["history", "export"])).stdout).unwrap();
    let dash: Value =
        serde_json::from_slice(&success(cli(&dir, &["history", "export", "-"])).stdout).unwrap();
    assert_eq!(stdout, dash);
    assert_eq!(stdout, json!({"version":1,"entries":listed}));
    let archive = dir.path().join("archive.json");
    success(cli(
        &dir,
        &[
            "history",
            "export",
            archive.to_str().unwrap(),
            "--key",
            "task/a",
        ],
    ));
    let archive_bytes = fs::read(&archive).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&archive_bytes).unwrap(),
        json!({"version":1,"entries":filtered})
    );
    assert!(
        !cli(&dir, &["history", "export", archive.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
    let normal: Value = serde_json::from_slice(&success(cli(&dir, &["export"])).stdout).unwrap();
    assert_eq!(normal["version"], 9);
    assert!(normal.get("history").is_none());
    assert_eq!(state(&storage, &path), before);
    assert_eq!(fs::read(sentinel.join("keep")).unwrap(), b"untouched");
    assert_eq!(fs::read_to_string(&config).unwrap(), "not valid toml [[[ ");
    drop(storage);
    success(cli(&dir, &["assign", "1", "local"]));
    let storage = SqliteStorage::open(&path).unwrap();
    assert_eq!(storage.history_entries(None).unwrap(), expected);
    let pending = storage.pending_changes().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].key, "task/a");
    assert_eq!(pending[0].value.as_ref().unwrap()["assigned_to"], "local");
    let import_dir = tempfile::tempdir().unwrap();
    assert!(
        !cli(&import_dir, &["import", archive.to_str().unwrap()])
            .status
            .success()
    );
}

#[test]
fn legacy_json_guard_applies_to_history_commands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("store.json"),
        r#"{"version":9,"entries":{}}"#,
    )
    .unwrap();
    let result = cli(&dir, &["history", "list", "--json"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("tdo migrate"));
    assert!(!dir.path().join("store.sqlite3").exists());
}
