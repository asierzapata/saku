use std::process::{Command, Output};

use saku_storage::timestamp::HybridTimestamp;
use saku_sync::replica::Replica;
use saku_tdo::models::{area::Area, project::Project, task::Task};
use saku_tdo::services::tasks::{AssignTaskParameters, assign_task};
use saku_tdo::storage::{
    Storage, StorageError,
    json::JsonFileStorage,
    sqlite::{CommandMode, SqliteStorage},
};
use serde_json::{Value, json};
use tempfile::TempDir;

fn command(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tdo"))
        .env("TDO_DATA_DIR", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env("TDO_NO_SYNC", "1")
        .env_remove("TDO_SYNC_DIR")
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn clock(value: &Value) -> HybridTimestamp {
    serde_json::from_value(value["modified_at"].clone()).unwrap()
}

fn future() -> Value {
    json!({"wall_ms":9000000000000_i64,"lamport":7,"device_id":"remote"})
}

fn metadata(path: &std::path::Path) -> (HybridTimestamp, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let (raw, generation): (String, i64) = connection
        .query_row(
            "SELECT clock, generation FROM store_metadata WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    (serde_json::from_str(&raw).unwrap(), generation)
}

fn contents(storage: &SqliteStorage) -> Value {
    serde_json::from_str(&storage.export_json().unwrap()).unwrap()
}

#[test]
fn local_edit_after_future_remote_wins_and_survives_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut task = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "important".into(),
        ..Task::default()
    })
    .unwrap();
    task["modified_at"] = future();
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(&json!({"version":9,"entries":{"task/a":task}}).to_string())
        .unwrap();
    drop(storage);
    success(command(&dir, &["assign", "1", "local"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    let edited = &exported["entries"]["task/a"];
    assert!(
        clock(edited) > clock(&task),
        "local={:?}, predecessor={:?}",
        clock(edited),
        clock(&task)
    );
    assert_eq!(edited["title"], "important");
    assert_eq!(edited["assigned_to"], "local");
    storage.prepare_sync("server").unwrap();
    storage
        .apply_pull("server", None, "1", &[("task/a".into(), task.to_string())])
        .unwrap();
    let result: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(result["entries"]["task/a"]["assigned_to"], "local");
    assert_eq!(result["entries"]["task/a"]["title"], "important");
}

#[test]
fn rename_stamps_tombstone_and_rewritten_references_after_future_clock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut area = serde_json::to_value(Area {
        name: "old".into(),
        ..Area::default()
    })
    .unwrap();
    let mut project = serde_json::to_value(Project {
        name: "project".into(),
        area_key: Some("area/old".into()),
        ..Project::default()
    })
    .unwrap();
    let mut task = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "important".into(),
        area_key: Some("area/old".into()),
        ..Task::default()
    })
    .unwrap();
    let unrelated = serde_json::to_value(Area {
        name: "unrelated".into(),
        ..Area::default()
    })
    .unwrap();
    for value in [&mut area, &mut project, &mut task] {
        value["modified_at"] = future();
    }
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage.import_json(&json!({"version":9,"entries":{"area/old":area,"area/unrelated":unrelated,"project/project":project,"task/a":task}}).to_string()).unwrap();
    drop(storage);
    success(command(&dir, &["edit", "area", "old", "--new-name", "new"]));
    let mut storage = SqliteStorage::open(path).unwrap();
    let result: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    let expected = clock(&area);
    for key in ["area/old", "area/new", "project/project", "task/a"] {
        assert!(
            clock(&result["entries"][key]) > expected,
            "{key}: {:?}",
            clock(&result["entries"][key])
        );
        assert_eq!(
            clock(&result["entries"][key]),
            clock(&result["entries"]["area/new"])
        );
    }
    let actor = std::fs::read_to_string(dir.path().join("saku/device_id")).unwrap();
    assert_eq!(
        clock(&result["entries"]["area/new"]).device_id,
        actor.trim()
    );
    assert!(result["entries"]["area/old"]["deleted_at"].is_string());
    assert_eq!(result["entries"]["area/old"]["renamed_to"], "area/new");
    assert_eq!(result["entries"]["project/project"]["area_key"], "area/new");
    assert_eq!(result["entries"]["task/a"]["area_key"], "area/new");
    assert_eq!(result["entries"]["area/unrelated"], unrelated);
    storage.prepare_sync("server").unwrap();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("area/old".into(), area.to_string()),
                ("project/project".into(), project.to_string()),
                ("task/a".into(), task.to_string()),
            ],
        )
        .unwrap();
    assert_eq!(contents(&storage), result);
}

#[test]
fn project_rename_stamps_all_changes_and_carries_lamport_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut project = serde_json::to_value(Project {
        name: "old".into(),
        ..Project::default()
    })
    .unwrap();
    let mut first = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "first".into(),
        project_key: Some("project/old".into()),
        ..Task::default()
    })
    .unwrap();
    let mut second = serde_json::to_value(Task {
        storage_key_suffix: "b".into(),
        task_number: 2,
        title: "second".into(),
        project_key: Some("project/old".into()),
        ..Task::default()
    })
    .unwrap();
    let unrelated = serde_json::to_value(Area {
        name: "unrelated".into(),
        ..Area::default()
    })
    .unwrap();
    let max = json!({"wall_ms":9000000000000_i64,"lamport":u64::MAX,"device_id":"remote"});
    for value in [&mut project, &mut first, &mut second] {
        value["modified_at"] = max.clone();
    }
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(
            &json!({"version":9,"entries":{
                "project/old":project,"task/a":first,"task/b":second,"area/unrelated":unrelated
            }})
            .to_string(),
        )
        .unwrap();
    drop(storage);
    success(command(
        &dir,
        &["edit", "project", "old", "--new-name", "new"],
    ));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let result = contents(&storage);
    let stamp = clock(&result["entries"]["project/new"]);
    assert_eq!((stamp.wall_ms, stamp.lamport), (9_000_000_000_001, 0));
    let actor = std::fs::read_to_string(dir.path().join("saku/device_id")).unwrap();
    assert_eq!(stamp.device_id, actor.trim());
    for key in ["project/old", "project/new", "task/a", "task/b"] {
        assert_eq!(clock(&result["entries"][key]), stamp, "{key}");
    }
    assert_eq!(
        result["entries"]["project/old"]["renamed_to"],
        "project/new"
    );
    assert!(result["entries"]["project/old"]["deleted_at"].is_string());
    assert_eq!(
        result["entries"]["project/new"]["previous_key"],
        "project/old"
    );
    assert_eq!(result["entries"]["task/a"]["project_key"], "project/new");
    assert_eq!(result["entries"]["task/b"]["project_key"], "project/new");
    assert_eq!(result["entries"]["area/unrelated"], unrelated);
    storage.prepare_sync("server").unwrap();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("project/old".into(), project.to_string()),
                ("task/a".into(), first.to_string()),
                ("task/b".into(), second.to_string()),
            ],
        )
        .unwrap();
    assert_eq!(contents(&storage), result);
}

#[test]
fn case_only_renames_update_display_names_without_self_links() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let area = serde_json::to_value(Area {
        name: "old".into(),
        ..Area::default()
    })
    .unwrap();
    let project = serde_json::to_value(Project {
        name: "project".into(),
        area_key: Some("area/old".into()),
        ..Project::default()
    })
    .unwrap();
    let task = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "important".into(),
        project_key: Some("project/project".into()),
        ..Task::default()
    })
    .unwrap();
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage.import_json(&json!({"version":9,"entries":{"area/old":area,"project/project":project,"task/a":task}}).to_string()).unwrap();
    drop(storage);
    success(command(&dir, &["edit", "area", "old", "--new-name", "OLD"]));
    success(command(
        &dir,
        &["edit", "project", "project", "--new-name", "PROJECT"],
    ));
    let storage = SqliteStorage::open(path).unwrap();
    let result = contents(&storage);
    assert_eq!(result["entries"].as_object().unwrap().len(), 3);
    assert_eq!(result["entries"]["area/old"]["name"], "OLD");
    assert_eq!(result["entries"]["project/project"]["name"], "PROJECT");
    for key in ["area/old", "project/project"] {
        assert!(result["entries"][key].get("previous_key").is_none());
        assert!(result["entries"][key].get("renamed_to").is_none());
        assert!(result["entries"][key]["deleted_at"].is_null());
    }
    assert_eq!(result["entries"]["project/project"]["area_key"], "area/old");
    assert_eq!(result["entries"]["task/a"], task);
    assert!(clock(&result["entries"]["area/old"]) > clock(&area));
    assert!(clock(&result["entries"]["project/project"]) > clock(&project));
}

#[test]
fn project_area_only_update_uses_one_new_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut project = serde_json::to_value(Project {
        name: "project".into(),
        ..Project::default()
    })
    .unwrap();
    let mut area = serde_json::to_value(Area {
        name: "work".into(),
        ..Area::default()
    })
    .unwrap();
    project["modified_at"] = future();
    area["modified_at"] = future();
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(
            &json!({"version":9,"entries":{"project/project":project,"area/work":area}})
                .to_string(),
        )
        .unwrap();
    drop(storage);
    success(command(
        &dir,
        &["edit", "project", "project", "--area", "work"],
    ));
    let storage = SqliteStorage::open(path).unwrap();
    let result = contents(&storage);
    assert_eq!(
        result["entries"]["project/project"]["area_key"],
        "area/work"
    );
    assert!(clock(&result["entries"]["project/project"]) > clock(&project));
    assert_eq!(result["entries"]["area/work"], area);
    assert_eq!(result["entries"].as_object().unwrap().len(), 2);
}

#[test]
fn repeated_processes_and_service_results_use_durable_clock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut original = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "important".into(),
        ..Task::default()
    })
    .unwrap();
    original["modified_at"] = future();
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(&json!({"version":9,"entries":{"task/a":original}}).to_string())
        .unwrap();
    drop(storage);
    let mut previous = clock(&original);
    for assignee in ["first", "second", "third"] {
        success(command(&dir, &["assign", "1", assignee]));
        let storage = SqliteStorage::open(&path).unwrap();
        let exported = contents(&storage);
        let entry = &exported["entries"]["task/a"];
        let stamp = clock(entry);
        assert!(stamp > previous);
        assert_eq!(entry["assigned_to"], assignee);
        assert_eq!(
            storage.pending_changes().unwrap()[0].value.as_ref(),
            Some(entry)
        );
        assert_eq!(metadata(&path).0, stamp);
        previous = stamp;
    }
    let mut storage = SqliteStorage::open(&path).unwrap();
    let returned = storage
        .transaction::<_, StorageError>(|store, tx| {
            assign_task(
                store,
                tx,
                AssignTaskParameters {
                    task_number_or_fuzzy_name: "1".into(),
                    assignee: Some("service".into()),
                },
            )
            .map_err(|error| StorageError::InvalidData(error.to_string()))
        })
        .unwrap();
    assert!(returned.modified_at > previous);
    let exported = contents(&storage);
    assert_eq!(returned.modified_at, clock(&exported["entries"]["task/a"]));
    assert_eq!(metadata(&path).0, returned.modified_at);
    assert_eq!(
        storage.pending_changes().unwrap()[0].value.as_ref(),
        Some(&exported["entries"]["task/a"])
    );
}

#[test]
fn observed_unrelated_remote_clock_advances_later_local_edits_without_echo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    success(command(&dir, &["add", "local task"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let before = storage.pending_changes().unwrap();
    storage.prepare_sync("server").unwrap();
    let mut remote = serde_json::to_value(Area {
        name: "remote".into(),
        ..Area::default()
    })
    .unwrap();
    remote["modified_at"] = future();
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("area/remote".into(), remote.to_string())],
        )
        .unwrap();
    let after = contents(&storage);
    assert_eq!(after["entries"]["area/remote"], remote);
    assert!(
        storage
            .pending_changes()
            .unwrap()
            .iter()
            .all(|pending| pending.key != "area/remote")
    );
    assert_eq!(storage.pending_changes().unwrap().len(), before.len());
    assert_eq!(metadata(&path).0, clock(&remote));
    drop(storage);
    success(command(&dir, &["assign", "1", "local"]));
    let storage = SqliteStorage::open(&path).unwrap();
    let after = contents(&storage);
    let task = after["entries"]
        .as_object()
        .unwrap()
        .values()
        .find(|value| value["title"] == "local task")
        .unwrap();
    assert!(clock(task) > clock(&remote));
    assert_eq!(task["assigned_to"], "local");
    assert_eq!(after["entries"]["area/remote"], remote);
}

#[test]
fn hard_removal_ack_and_reopen_keep_clock_floor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut value = serde_json::to_value(Task {
        storage_key_suffix: "a".into(),
        task_number: 1,
        title: "removed".into(),
        ..Task::default()
    })
    .unwrap();
    value["modified_at"] = future();
    let floor = clock(&value);
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage
        .import_json(&json!({"version":9,"entries":{"task/a":value}}).to_string())
        .unwrap();
    storage
        .transaction::<_, StorageError>(|store, _| {
            store.tasks.remove("task/a");
            Ok(())
        })
        .unwrap();
    let pending = storage.pending_changes().unwrap();
    assert_eq!(pending[0].value, None);
    storage
        .acknowledge_changes(
            &pending
                .iter()
                .map(|p| (p.key.clone(), p.revision))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    drop(storage);
    assert_eq!(metadata(&path).0, floor);
    success(command(&dir, &["add", "after removal"]));
    let storage = SqliteStorage::open(path).unwrap();
    let task = storage
        .load()
        .unwrap()
        .tasks
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(task.modified_at > floor);
}

#[test]
fn failed_transaction_and_failed_page_preserve_clock_data_and_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    success(command(&dir, &["add", "original"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let before = contents(&storage);
    let pending = storage.pending_changes().unwrap();
    let state = metadata(&path);
    let result = storage.transaction::<(), StorageError>(|store, tx| {
        let stamp = tx.next_modified_at()?;
        let task = store.tasks.values_mut().next().unwrap();
        task.title = "rolled back".into();
        task.modified_at = stamp;
        tx.save(store)?;
        Err(StorageError::InvalidData("injected failure".into()))
    });
    assert!(result.is_err());
    assert_eq!(contents(&storage), before);
    assert_eq!(storage.pending_changes().unwrap(), pending);
    assert_eq!(metadata(&path), state);
    storage.prepare_sync("server").unwrap();
    let state = metadata(&path);
    let before = contents(&storage);
    let pending = storage.pending_changes().unwrap();
    let mut good = serde_json::to_value(Area {
        name: "new".into(),
        ..Area::default()
    })
    .unwrap();
    good["modified_at"] = future();
    assert!(
        storage
            .apply_pull(
                "server",
                None,
                "1",
                &[
                    ("area/new".into(), good.to_string()),
                    (
                        "task/bad".into(),
                        r#"{"modified_at":{"wall_ms":1,"wall_ms":2}}"#.into()
                    ),
                ]
            )
            .is_err()
    );
    assert_eq!(metadata(&path), state);
    assert_eq!(contents(&storage), before);
    assert_eq!(storage.pending_changes().unwrap(), pending);
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
}

#[test]
fn reads_and_noop_transaction_do_not_advance_clock_or_generation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    success(command(&dir, &["add", "task"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let before = metadata(&path);
    storage.load().unwrap();
    storage.export_json().unwrap();
    storage
        .transaction::<_, StorageError>(|_, _| Ok(()))
        .unwrap();
    assert_eq!(metadata(&path), before);
    let read = storage.command(CommandMode::Read).unwrap();
    assert!(read.next_modified_at().is_err());
    read.commit().unwrap();
    assert_eq!(metadata(&path), before);
}

#[test]
fn edit_reservations_are_not_persisted_and_clock_only_changes_are_stale() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    success(command(&dir, &["add", "task"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let initial = metadata(&path);
    {
        let edit = storage.command(CommandMode::Edit).unwrap();
        let first = edit.next_modified_at().unwrap();
        let second = edit.next_modified_at().unwrap();
        assert!(second > first && first > initial.0);
        edit.commit().unwrap();
    }
    assert_eq!(metadata(&path), initial);
    let mut other = SqliteStorage::open(&path).unwrap();
    let edit = storage.command(CommandMode::Edit).unwrap();
    let mut snapshot = edit.load().unwrap();
    let reserved = edit.next_modified_at().unwrap();
    let task = snapshot.tasks.values_mut().next().unwrap();
    task.title = "stale edit".into();
    task.modified_at = reserved;
    let write = other.command(CommandMode::Write).unwrap();
    let committed = write.next_modified_at().unwrap();
    write.commit().unwrap();
    let changed = metadata(&path);
    assert_eq!(changed.1, initial.1);
    assert_eq!(changed.0, committed);
    assert!(
        edit.save(&snapshot)
            .unwrap_err()
            .to_string()
            .contains("store changed while editing")
    );
    edit.commit().unwrap();
    assert_eq!(metadata(&path), changed);
    assert_eq!(
        other.load().unwrap().tasks.values().next().unwrap().title,
        "task"
    );
}

#[test]
fn successful_optimistic_edit_persists_reserved_stamp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    success(command(&dir, &["add", "task"]));
    let mut storage = SqliteStorage::open(&path).unwrap();
    let before = metadata(&path);
    let edit = storage.command(CommandMode::Edit).unwrap();
    let mut snapshot = edit.load().unwrap();
    let stamp = edit.next_modified_at().unwrap();
    assert_eq!(metadata(&path), before);
    let task = snapshot.tasks.values_mut().next().unwrap();
    task.title = "edited".into();
    task.modified_at = stamp.clone();
    edit.save(&snapshot).unwrap();
    edit.commit().unwrap();
    assert_eq!(metadata(&path), (stamp.clone(), before.1 + 1));
    let stored = storage.load().unwrap();
    let task = stored.tasks.values().next().unwrap();
    assert_eq!(task.title, "edited");
    assert_eq!(task.modified_at, stamp);
    assert_eq!(
        storage.pending_changes().unwrap()[0]
            .value
            .as_ref()
            .unwrap()["modified_at"],
        serde_json::to_value(stamp).unwrap()
    );
}

#[test]
fn sqlite_requires_a_real_device_id_and_rolls_back_failure() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "file").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tdo"))
        .env("TDO_DATA_DIR", dir.path())
        .env("XDG_DATA_HOME", blocker)
        .env("XDG_CONFIG_HOME", dir.path())
        .env("TDO_NO_SYNC", "1")
        .args(["add", "must fail"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("device ID"));
    let path = dir.path().join("store.sqlite3");
    let storage = SqliteStorage::open(&path).unwrap();
    assert!(storage.load().unwrap().tasks.is_empty());
    assert!(storage.pending_changes().unwrap().is_empty());
    assert_eq!(metadata(&path), (HybridTimestamp::default(), 0));
}

#[test]
fn json_storage_uses_default_clock_method_for_services() {
    let dir = tempfile::tempdir().unwrap();
    let storage = JsonFileStorage::new(dir.path().join("history.json"));
    let mut store = storage.load().unwrap();
    store.add_task(Task {
        storage_key_suffix: "a".into(),
        title: "json".into(),
        ..Task::default()
    });
    let returned = assign_task(
        &mut store,
        &storage,
        AssignTaskParameters {
            task_number_or_fuzzy_name: "1".into(),
            assignee: Some("local".into()),
        },
    )
    .unwrap();
    assert_eq!(returned.assigned_to.as_deref(), Some("local"));
    assert_eq!(
        storage.load().unwrap().tasks["task/a"].modified_at,
        returned.modified_at
    );
}
