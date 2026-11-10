use std::fs;
use std::process::{Command, Output};

use saku_tdo::models::{area::Area, project::Project, store::Store, task::Task};
use saku_tdo::storage::sqlite::SqliteStorage;
use tempfile::TempDir;

fn command(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tdo"));
    command
        .env("TDO_DATA_DIR", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env("TDO_NO_SYNC", "1")
        .env_remove("TDO_SYNC_DIR");
    command
}

fn run(dir: &TempDir, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
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

fn db_clock(dir: &TempDir) -> String {
    rusqlite::Connection::open(dir.path().join("store.sqlite3"))
        .unwrap()
        .query_row("SELECT clock FROM store_metadata WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn fixture() -> String {
    let mut store = Store::default();
    store.add_task(Task {
        storage_key_suffix: "original".into(),
        title: "Legacy task".into(),
        ..Task::default()
    });
    store.add_area(Area {
        name: "old".into(),
        ..Area::default()
    });
    store.add_area(Area {
        name: "new".into(),
        ..Area::default()
    });
    store.add_project(Project {
        name: "project".into(),
        area_key: Some("area/old".into()),
        ..Project::default()
    });
    let mut value = serde_json::to_value(store.to_stored()).unwrap();
    value["entries"]["area/old"]["deleted_at"] = serde_json::json!("2026-01-01T00:00:00Z");
    value["entries"]["area/old"]["renamed_to"] = serde_json::json!("area/new");
    value["entries"]["area/new"]["previous_key"] = serde_json::json!("area/old");
    value["entries"]["task/original"]["project_key"] = serde_json::json!("project/project");
    value["entries"]["task/original"]["when"] = serde_json::json!({"type":"Today","evening":false});
    value["entries"]["task/original"]["notes"] = serde_json::json!("legacy note");
    value["entries"]["task/original"]["depends_on"] = serde_json::json!(["task/missing"]);
    value["entries"]["task/original"]["recurrence"] = serde_json::json!({
        "freq":"weekly", "weekdays":[], "monthly_anchor":null, "until":null, "dtstart":"2026-03-10"
    });
    serde_json::to_string_pretty(&value).unwrap()
}

fn assert_normalized(
    source: &str,
    exported: &[u8],
    before: jiff::civil::Date,
    after: jiff::civil::Date,
) {
    let actual: serde_json::Value = serde_json::from_slice(exported).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(source).unwrap();
    let when = actual["entries"]["task/original"]["when"].clone();
    assert_eq!(when["type"], "Scheduled");
    let date = when["date"]
        .as_str()
        .unwrap()
        .parse::<jiff::civil::Date>()
        .unwrap();
    assert!(date == before || date == after);
    expected["entries"]["task/original"]["when"] =
        serde_json::json!({"type":"Scheduled", "date":date.to_string()});
    assert_eq!(actual, expected);
}

#[test]
fn local_add_and_view_use_sqlite_without_legacy_sync() {
    let dir = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let output = command(&dir)
        .env_remove("TDO_NO_SYNC")
        .env("TDO_SYNC_DIR", remote.path())
        .args(["add", "local task"])
        .output()
        .unwrap();
    success(output);
    let path = dir.path().join("store.sqlite3");
    assert!(path.exists());
    assert!(!dir.path().join("store.json").exists());
    assert!(!dir.path().join("dirty.json").exists());
    assert_eq!(fs::read_dir(remote.path()).unwrap().count(), 0);
    let storage = SqliteStorage::open(path).unwrap();
    assert_eq!(storage.pending_changes().unwrap().len(), 1);
    let view = success(run(&dir, &["view", "inbox", "--json"]));
    let tasks: serde_json::Value = serde_json::from_slice(&view.stdout).unwrap();
    assert_eq!(tasks[0]["title"], "local task");
}

#[test]
fn concurrent_cli_adds_do_not_overwrite_or_duplicate_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let mut children = Vec::new();
    for index in 0..12 {
        children.push(
            command(&dir)
                .args(["add", &format!("task {index}")])
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let storage = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
    let store = storage.load().unwrap();
    assert_eq!(store.tasks.len(), 12);
    assert_eq!(storage.pending_changes().unwrap().len(), 12);
    let titles: std::collections::BTreeSet<_> = store
        .tasks
        .values()
        .map(|task| task.title.clone())
        .collect();
    let expected: std::collections::BTreeSet<_> =
        (0..12).map(|index| format!("task {index}")).collect();
    assert_eq!(titles, expected);
    let mut previous = None;
    for number in 1..=12 {
        let stamp = &store.get_task_by_number(number).unwrap().modified_at;
        if let Some(previous) = previous {
            assert!(stamp > previous);
        }
        previous = Some(stamp);
    }
    for key in store.tasks.keys() {
        let suffix = key.strip_prefix("task/").unwrap();
        assert_eq!(suffix.len(), 32);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn multi_done_error_rolls_back_entries_and_outbox() {
    let dir = tempfile::tempdir().unwrap();
    success(run(&dir, &["add", "first"]));
    let storage = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
    let before = storage.pending_changes().unwrap();
    let clock_before = db_clock(&dir);
    let snapshot: serde_json::Value =
        serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    drop(storage);
    assert!(!run(&dir, &["done", "1", "999"]).status.success());
    let storage = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
    assert_eq!(storage.pending_changes().unwrap(), before);
    assert_eq!(db_clock(&dir), clock_before);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&storage.export_json().unwrap()).unwrap(),
        snapshot
    );
}

#[test]
fn legacy_guard_and_explicit_migration_keep_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = fixture();
    let path = dir.path().join("store.json");
    fs::write(&path, &source).unwrap();
    for args in [
        &["add", "not migrated"][..],
        &["view", "inbox", "--json"][..],
    ] {
        let result = run(&dir, args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("tdo migrate"));
    }
    assert!(!path.with_file_name("store.sqlite3").exists());
    fs::write(&path, r#"{"version":9,"version":9,"entries":{}}"#).unwrap();
    assert!(!run(&dir, &["migrate"]).status.success());
    assert!(!run(&dir, &["add", "still blocked"]).status.success());
    fs::write(&path, &source).unwrap();
    let before = jiff::Zoned::now().date();
    success(run(&dir, &["migrate"]));
    let after = jiff::Zoned::now().date();
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_normalized(
        &source,
        &success(run(&dir, &["export"])).stdout,
        before,
        after,
    );
    fs::write(&path, "changed legacy data").unwrap();
    success(run(&dir, &["add", "sqlite authoritative"]));
    assert!(!run(&dir, &["migrate"]).status.success());
}

#[test]
fn legacy_guard_does_not_upgrade_a_pristine_v1_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    drop(SqliteStorage::open(&path).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE recovery_history; DROP TABLE sync_state; ALTER TABLE store_metadata DROP COLUMN clock; PRAGMA user_version = 1;")
        .unwrap();
    drop(connection);
    fs::write(dir.path().join("store.json"), fixture()).unwrap();
    let result = run(&dir, &["add", "blocked"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("tdo migrate"));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let objects: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'sync_state'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(objects, 0);
}

#[test]
fn empty_import_consumes_pristine_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.json");
    fs::write(&path, r#"{"version":9,"entries":{}}"#).unwrap();
    success(run(&dir, &["import", path.to_str().unwrap()]));
    assert!(
        !run(&dir, &["import", path.to_str().unwrap()])
            .status
            .success()
    );
}

#[test]
fn failed_imports_leave_target_available_for_valid_import() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.json");
    for bad in [
        r#"{"version":10,"entries":{}}"#,
        r#"{"version":8,"entries":{}}"#,
        r#"{"version":9,"version":9,"entries":{}}"#,
        r#"{"version":9,"entries":{"task/bad":"invalid"}}"#,
    ] {
        fs::write(&source, bad).unwrap();
        assert!(
            !run(&dir, &["import", source.to_str().unwrap()])
                .status
                .success()
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), bad);
    }
    let valid = fixture();
    fs::write(&source, &valid).unwrap();
    let before = jiff::Zoned::now().date();
    success(run(&dir, &["import", source.to_str().unwrap()]));
    let after = jiff::Zoned::now().date();
    assert_eq!(fs::read_to_string(&source).unwrap(), valid);
    let exported = success(run(&dir, &["export"])).stdout;
    assert_normalized(&valid, &exported, before, after);
    assert!(
        !run(&dir, &["import", source.to_str().unwrap()])
            .status
            .success()
    );
}

#[test]
fn export_is_portable_and_never_overwrites_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    success(run(&dir, &["add", "exported task"]));
    let exported = success(run(&dir, &["export"])).stdout;
    let source = other.path().join("input.json");
    fs::write(&source, &exported).unwrap();
    success(run(&other, &["import", source.to_str().unwrap()]));
    let restored = success(run(&other, &["export"])).stdout;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&exported).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&restored).unwrap()
    );
    let sentinel = dir.path().join("sentinel");
    let legacy = dir.path().join("store.json");
    fs::write(&sentinel, b"keep").unwrap();
    fs::write(&legacy, b"legacy sentinel").unwrap();
    for path in [&sentinel, &legacy, &dir.path().join("store.sqlite3")] {
        assert!(
            !run(&dir, &["export", path.to_str().unwrap()])
                .status
                .success()
        );
    }
    assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
    assert_eq!(fs::read(&legacy).unwrap(), b"legacy sentinel");
}

#[test]
fn import_accepts_stdin_and_export_dash_is_json() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let source = fixture();
    let before = jiff::Zoned::now().date();
    let mut child = command(&dir)
        .args(["import", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    success(child.wait_with_output().unwrap());
    let after = jiff::Zoned::now().date();
    let export = success(run(&dir, &["export", "-"]));
    assert_normalized(&source, &export.stdout, before, after);
}

#[test]
fn completion_never_creates_data() {
    let dir = tempfile::tempdir().unwrap();
    success(run(&dir, &["completion", "bash"]));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn editor_does_not_lock_writer_and_refuses_stale_save() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    success(run(&dir, &["add", "original"]));
    let ready = dir.path().join("ready");
    let fifo = dir.path().join("go");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let editor = dir.path().join("editor");
    fs::write(&editor, "#!/bin/sh\n: > \"$READY\"\nread input < \"$GO\"\ncontent=$(sed 's/^Title: .*/Title: edited/' \"$1\")\nprintf '%s\\n' \"$content\" > \"$1\"\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = command(&dir)
        .env("VISUAL", &editor)
        .env("READY", &ready)
        .env("GO", &fifo)
        .args(["edit", "task", "1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("editor exited before readiness: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("editor did not become ready");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    success(run(&dir, &["add", "concurrent"]));
    let clock_after_writer = db_clock(&dir);
    writeln!(
        fs::OpenOptions::new().write(true).open(&fifo).unwrap(),
        "go"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("store changed while editing"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let storage = SqliteStorage::open(dir.path().join("store.sqlite3")).unwrap();
    let store = storage.load().unwrap();
    assert_eq!(store.get_task_by_number(1).unwrap().title, "original");
    assert_eq!(store.get_task_by_number(2).unwrap().title, "concurrent");
    assert_eq!(db_clock(&dir), clock_after_writer);
    let unchanged = command(&dir)
        .env("VISUAL", "/bin/true")
        .args(["edit", "task", "1"])
        .output()
        .unwrap();
    success(unchanged);
    assert_eq!(db_clock(&dir), clock_after_writer);
}
