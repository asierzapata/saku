use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use saku_crypto::{MasterKey, encrypt_entry, kdf::derive_master_key};
use saku_sync::SyncError;
use saku_sync::replica::{PullPage, RemoteEntry, Replica, Transport, sync_replica};
use saku_tdo::models::{area::Area, task::Task};
use saku_tdo::storage::{StorageError, sqlite::SqliteStorage};
use serde_json::{Value, json};

fn key() -> &'static MasterKey {
    static KEY: OnceLock<MasterKey> = OnceLock::new();
    KEY.get_or_init(|| derive_master_key(b"isolated test key", &[7; 16]).unwrap())
}

fn task(suffix: &str, number: u64, title: &str, wall_ms: i64) -> Value {
    let mut task = Task {
        storage_key_suffix: suffix.into(),
        task_number: number,
        title: title.into(),
        ..Task::default()
    };
    task.modified_at.wall_ms = wall_ms;
    serde_json::to_value(task).unwrap()
}

fn remote(key_name: &str, text: &str) -> RemoteEntry {
    RemoteEntry {
        key: key_name.into(),
        blob: encrypt_entry(text.as_bytes(), key()),
        deleted: false,
    }
}

fn page(cookie: &str, more: bool, entries: Vec<RemoteEntry>) -> PullPage {
    PullPage {
        entries,
        cookie: cookie.into(),
        has_more: more,
    }
}

#[derive(Default)]
struct FakeTransport {
    pages: VecDeque<PullPage>,
    pull_cookies: Vec<Option<String>>,
    base_cookies: Vec<String>,
    pushed: Vec<Vec<(String, Vec<u8>)>>,
    bad_ack: bool,
    fail_push: bool,
    before_pull: Option<Box<dyn FnOnce()>>,
    before_push: Option<Box<dyn FnOnce()>>,
}

impl Transport for FakeTransport {
    fn pull(&mut self, cookie: Option<&str>) -> Result<PullPage, SyncError> {
        self.pull_cookies.push(cookie.map(str::to_owned));
        if let Some(callback) = self.before_pull.take() {
            callback();
        }
        self.pages.pop_front().ok_or_else(|| SyncError::Backend {
            message: "fake pull failure".into(),
        })
    }

    fn push(&mut self, entries: &[(String, Vec<u8>)], base_cookie: &str) -> Result<Vec<String>, SyncError> {
        self.base_cookies.push(base_cookie.to_owned());
        if let Some(callback) = self.before_push.take() {
            callback();
        }
        self.pushed.push(entries.to_vec());
        if self.fail_push {
            return Err(SyncError::Backend {
                message: "fake push failure".into(),
            });
        }
        if self.bad_ack {
            return Ok(vec!["task/unknown".into()]);
        }
        Ok(entries.iter().map(|(key, _)| key.clone()).collect())
    }
}

fn add(path: &Path, suffix: &str, title: &str) {
    let mut storage = SqliteStorage::open(path).unwrap();
    storage
        .transaction::<_, StorageError>(|store, _| {
            store.add_task(Task {
                storage_key_suffix: suffix.into(),
                title: title.into(),
                ..Task::default()
            });
            Ok(())
        })
        .unwrap();
}

fn write_title(path: PathBuf, title: &'static str) {
    let mut storage = SqliteStorage::open(path).unwrap();
    storage
        .transaction::<_, StorageError>(|store, _| {
            let task = store.tasks.get_mut("task/a").unwrap();
            task.title = title.into();
            task.modified_at.wall_ms += 1000;
            Ok(())
        })
        .unwrap();
}

#[test]
fn remote_winner_does_not_echo_and_local_winner_heals() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(&path).unwrap();
    storage.prepare_sync("server").unwrap();
    let newer = task("a", 1, "remote", 100);
    storage
        .apply_pull("server", None, "1", &[("task/a".into(), newer.to_string())])
        .unwrap();
    assert_eq!(storage.load().unwrap().tasks["task/a"].title, "remote");
    assert!(storage.pending_changes().unwrap().is_empty());
    let older = task("a", 1, "stale", 50);
    storage
        .apply_pull(
            "server",
            Some("1"),
            "2",
            &[("task/a".into(), older.to_string())],
        )
        .unwrap();
    let pending = storage.pending_changes().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].value.as_ref().unwrap()["title"], "remote");
}

#[test]
fn equal_clock_legacy_schedule_cannot_override_stable_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut local = task("a", 1, "local", 100);
    local["when"] = json!({"type":"Scheduled","date":"2020-01-01"});
    storage
        .import_json(&json!({"version":9,"entries":{"task/a":local}}).to_string())
        .unwrap();
    storage.prepare_sync("server").unwrap();
    let pending = storage.pending_changes().unwrap();
    storage
        .acknowledge_changes(
            &pending
                .iter()
                .map(|p| (p.key.clone(), p.revision))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut remote = task("a", 10, "remote", 100);
    remote["when"] = json!({"type":"Today","evening":false});
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let result: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(
        result["entries"]["task/a"]["when"],
        json!({"type":"Scheduled","date":"2020-01-01"})
    );
    assert_eq!(result["entries"]["task/a"]["title"], "local");
    assert_eq!(result["entries"]["task/a"]["task_number"], 1);
    let queued = storage.pending_changes().unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued[0].value.as_ref().unwrap(),
        &result["entries"]["task/a"]
    );
    storage
        .apply_pull(
            "server",
            Some("1"),
            "2",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    assert_eq!(storage.pending_changes().unwrap(), queued);
    assert_eq!(
        serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
        result
    );

    remote["modified_at"]["wall_ms"] = json!(101);
    let before = jiff::Zoned::now().date();
    storage
        .apply_pull(
            "server",
            Some("2"),
            "3",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let after = jiff::Zoned::now().date();
    let today: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(today["entries"]["task/a"]["title"], "remote");
    assert_eq!(today["entries"]["task/a"]["when"]["type"], "Scheduled");
    let date = today["entries"]["task/a"]["when"]["date"]
        .as_str()
        .unwrap()
        .parse::<jiff::civil::Date>()
        .unwrap();
    assert!(date == before || date == after);

    remote["modified_at"]["wall_ms"] = json!(102);
    remote["when"] = json!({"type":"Anytime"});
    storage
        .apply_pull(
            "server",
            Some("3"),
            "4",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    let anytime: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(
        anytime["entries"]["task/a"]["when"],
        json!({"type":"Someday"})
    );
    let pending = storage.pending_changes().unwrap();
    storage
        .apply_pull(
            "server",
            Some("4"),
            "5",
            &[("task/a".into(), remote.to_string())],
        )
        .unwrap();
    assert_eq!(storage.pending_changes().unwrap(), pending);
    assert_eq!(
        serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
        anytime
    );
}

#[test]
fn duplicate_numbers_are_fixed_deterministically_on_both_devices() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    add(&a, "a", "first");
    add(&b, "b", "second");
    let mut first = SqliteStorage::open(a).unwrap();
    let mut second = SqliteStorage::open(b).unwrap();
    let a_json = first.pending_changes().unwrap()[0]
        .value
        .clone()
        .unwrap()
        .to_string();
    let b_json = second.pending_changes().unwrap()[0]
        .value
        .clone()
        .unwrap()
        .to_string();
    first.prepare_sync("server").unwrap();
    second.prepare_sync("server").unwrap();
    first
        .apply_pull("server", None, "1", &[("task/b".into(), b_json)])
        .unwrap();
    second
        .apply_pull("server", None, "1", &[("task/a".into(), a_json)])
        .unwrap();
    let left = first.load().unwrap();
    let right = second.load().unwrap();
    assert_eq!(
        left.tasks["task/a"].task_number,
        right.tasks["task/a"].task_number
    );
    assert_eq!(
        left.tasks["task/b"].task_number,
        right.tasks["task/b"].task_number
    );
    assert_ne!(
        left.tasks["task/a"].task_number,
        left.tasks["task/b"].task_number
    );
}

#[test]
fn invalid_remote_page_and_stale_cursor_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    storage.prepare_sync("server").unwrap();
    let valid = task("a", 1, "valid", 1).to_string();
    let duplicate = valid.replacen("\"title\":", "\"title\":\"discarded\",\"title\":", 1);
    assert!(
        storage
            .apply_pull(
                "server",
                None,
                "1",
                &[
                    ("task/a".into(), valid.clone()),
                    ("task/b".into(), duplicate)
                ]
            )
            .is_err()
    );
    assert!(storage.load().unwrap().tasks.is_empty());
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
    assert!(storage.apply_pull("other", None, "1", &[]).is_err());
    assert!(
        storage
            .apply_pull("server", Some("stale"), "1", &[])
            .is_err()
    );
    assert!(
        storage
            .apply_pull(
                "server",
                None,
                "1",
                &[("task/a".into(), valid.clone()), ("task/a".into(), valid)]
            )
            .is_err()
    );
    assert!(storage.prepare_sync("other").is_err());
}

#[test]
fn bad_ciphertext_and_hard_deletes_do_not_advance_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut transport = FakeTransport {
        pages: VecDeque::from([page(
            "1",
            false,
            vec![RemoteEntry {
                key: "task/a".into(),
                blob: vec![0],
                deleted: false,
            }],
        )]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut transport, key(), "server").is_err());
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
    assert!(storage.history_entries(None).unwrap().is_empty());
    assert!(transport.pushed.is_empty());
    let mut transport = FakeTransport {
        pages: VecDeque::from([page(
            "1",
            false,
            vec![RemoteEntry {
                key: "task/a".into(),
                blob: vec![],
                deleted: true,
            }],
        )]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut transport, key(), "server").is_err());
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
    assert!(storage.history_entries(None).unwrap().is_empty());
    assert!(transport.pushed.is_empty());
}

#[test]
fn ciphertext_failure_preserves_history_and_never_pushes() {
    for fail_later in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        add(&path, "a", "local");
        let mut storage = SqliteStorage::open(&path).unwrap();
        storage.prepare_sync("server").unwrap();
        let before = storage.export_json().unwrap();
        let local: Value =
            serde_json::from_str::<Value>(&before).unwrap()["entries"]["task/a"].clone();
        let pending = storage.pending_changes().unwrap();
        let changed = task("a", 1, "remote", 100);
        let good = remote("task/a", &changed.to_string());
        let bad = RemoteEntry {
            key: "task/b".into(),
            blob: vec![0],
            deleted: false,
        };
        let mut transport = if fail_later {
            FakeTransport {
                pages: VecDeque::from([
                    page("1", true, vec![good.clone()]),
                    page("2", false, vec![good, bad]),
                ]),
                ..Default::default()
            }
        } else {
            FakeTransport {
                pages: VecDeque::from([page("1", false, vec![good, bad])]),
                ..Default::default()
            }
        };
        assert!(sync_replica(&mut storage, &mut transport, key(), "server").is_err());
        assert!(transport.pushed.is_empty());
        if fail_later {
            assert_eq!(
                storage.prepare_sync("server").unwrap().as_deref(),
                Some("1")
            );
            let history = storage.history_entries(None).unwrap();
            assert_eq!(history.len(), 2);
            assert_eq!(history[0].value, local);
            assert_eq!(history[1].value, changed);
            assert!(storage.pending_changes().unwrap().is_empty());
            assert_eq!(storage.load().unwrap().tasks["task/a"].title, "remote");
        } else {
            assert_eq!(storage.prepare_sync("server").unwrap(), None);
            assert_eq!(storage.export_json().unwrap(), before);
            assert_eq!(storage.pending_changes().unwrap(), pending);
            assert!(storage.history_entries(None).unwrap().is_empty());
        }
    }
}

#[test]
fn ack_validation_and_push_retry_preserve_pending_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(path).unwrap();
    let mut bad = FakeTransport {
        pages: VecDeque::from([page("1", false, vec![])]),
        bad_ack: true,
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut bad, key(), "server").is_err());
    let before = storage.pending_changes().unwrap();
    let mut fail = FakeTransport {
        pages: VecDeque::from([page("2", false, vec![])]),
        fail_push: true,
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut fail, key(), "server").is_err());
    assert_eq!(storage.pending_changes().unwrap(), before);
    let mut retry = FakeTransport {
        pages: VecDeque::from([page("3", false, vec![])]),
        ..Default::default()
    };
    let outcome = sync_replica(&mut storage, &mut retry, key(), "server").unwrap();
    assert_eq!(outcome.pushed, 1);
    assert!(storage.pending_changes().unwrap().is_empty());
    assert_eq!(
        storage.prepare_sync("server").unwrap().as_deref(),
        Some("3")
    );
}

#[test]
fn concurrent_edit_during_push_survives_stale_ack() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(&path).unwrap();
    let mut transport = FakeTransport {
        pages: VecDeque::from([page("1", false, vec![])]),
        before_push: Some(Box::new(move || write_title(path, "concurrent"))),
        ..Default::default()
    };
    sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
    let pending = storage.pending_changes().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].value.as_ref().unwrap()["title"], "concurrent");
}

#[test]
fn concurrent_edit_during_pull_is_merged_from_latest_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(&path).unwrap();
    let older = task("a", 1, "remote", 10);
    let mut transport = FakeTransport {
        pages: VecDeque::from([page("1", false, vec![remote("task/a", &older.to_string())])]),
        before_pull: Some(Box::new(move || write_title(path, "concurrent"))),
        ..Default::default()
    };
    sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
    assert_eq!(storage.load().unwrap().tasks["task/a"].title, "concurrent");
    assert_eq!(transport.pushed.len(), 1);
}

#[test]
fn pagination_resumes_committed_page_and_rejects_no_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let a = task("a", 1, "first", 1);
    let mut first = FakeTransport {
        pages: VecDeque::from([page("1", true, vec![remote("task/a", &a.to_string())])]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut first, key(), "server").is_err());
    assert_eq!(
        storage.prepare_sync("server").unwrap().as_deref(),
        Some("1")
    );
    assert_eq!(storage.load().unwrap().tasks.len(), 1);
    let mut stagnant = FakeTransport {
        pages: VecDeque::from([page("1", true, vec![])]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut stagnant, key(), "server").is_err());
    let mut retry = FakeTransport {
        pages: VecDeque::from([page("2", false, vec![])]),
        ..Default::default()
    };
    sync_replica(&mut storage, &mut retry, key(), "server").unwrap();
    assert_eq!(retry.pull_cookies, vec![Some("1".to_string())]);
}

#[test]
fn hard_deletion_remains_pending_and_is_not_pushed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(path).unwrap();
    storage
        .transaction::<_, StorageError>(|store, _| {
            store.tasks.remove("task/a");
            Ok(())
        })
        .unwrap();
    let mut transport = FakeTransport {
        pages: VecDeque::from([page("1", false, vec![])]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut transport, key(), "server").is_err());
    assert_eq!(storage.pending_changes().unwrap()[0].value, None);
    assert!(transport.pushed.is_empty());
    let remote_value = task("a", 1, "remote", 100);
    let mut transport = FakeTransport {
        pages: VecDeque::from([page(
            "2",
            false,
            vec![remote("task/a", &remote_value.to_string())],
        )]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut transport, key(), "server").is_err());
    assert_eq!(storage.pending_changes().unwrap()[0].value, None);
    assert!(storage.load().unwrap().tasks.is_empty());
    assert_eq!(
        storage.prepare_sync("server").unwrap().as_deref(),
        Some("1")
    );
    storage.reset_sync("server", Some("1")).unwrap();
    assert_eq!(storage.prepare_sync("server").unwrap(), None);
    assert_eq!(storage.pending_changes().unwrap()[0].value, None);
    let mut recovery = FakeTransport {
        pages: VecDeque::from([page("0", false, vec![])]),
        ..Default::default()
    };
    assert!(sync_replica(&mut storage, &mut recovery, key(), "server").is_err());
    assert!(recovery.pushed.is_empty());
    assert_eq!(storage.pending_changes().unwrap()[0].value, None);
}

#[test]
fn rename_repairs_references_and_queues_repaired_local_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    let mut local = task("a", 1, "local", 1);
    local["area_key"] = json!("area/old");
    storage
        .import_json(&json!({"version":9, "entries":{"task/a":local}}).to_string())
        .unwrap();
    let previous = storage.pending_changes().unwrap();
    storage
        .acknowledge_changes(
            &previous
                .iter()
                .map(|entry| (entry.key.clone(), entry.revision))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert!(storage.pending_changes().unwrap().is_empty());
    storage.prepare_sync("server").unwrap();
    assert_eq!(storage.pending_changes().unwrap().len(), 1);
    let old = Area {
        name: "old".into(),
        ..Area::default()
    };
    let new = Area {
        name: "new".into(),
        ..Area::default()
    };
    let mut tombstone = serde_json::to_value(old).unwrap();
    tombstone["deleted_at"] = json!("2026-01-01T00:00:00Z");
    tombstone["renamed_to"] = json!("area/new");
    let mut replacement = serde_json::to_value(new).unwrap();
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
    let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
    assert_eq!(exported["entries"]["task/a"]["area_key"], "area/new");
    assert_eq!(exported["entries"]["area/old"]["renamed_to"], "area/new");
    assert!(!exported["entries"]["area/old"]["deleted_at"].is_null());
    let pending = storage.pending_changes().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].key, "task/a");
    assert_eq!(pending[0].value.as_ref().unwrap()["area_key"], "area/new");
}

#[test]
fn duplicate_number_overflow_rolls_back_page_but_unique_max_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    storage.prepare_sync("server").unwrap();
    let a = task("a", u64::MAX, "max", 1);
    storage
        .apply_pull("server", None, "1", &[("task/a".into(), a.to_string())])
        .unwrap();
    let before = storage.pending_changes().unwrap();
    let b = task("b", u64::MAX, "duplicate", 1);
    assert!(
        storage
            .apply_pull(
                "server",
                Some("1"),
                "2",
                &[("task/b".into(), b.to_string())]
            )
            .is_err()
    );
    assert_eq!(
        storage.prepare_sync("server").unwrap().as_deref(),
        Some("1")
    );
    assert_eq!(storage.load().unwrap().tasks.len(), 1);
    assert_eq!(storage.pending_changes().unwrap(), before);
}

#[test]
fn duplicate_number_order_uses_timestamp_instants_not_date_string_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
    storage.prepare_sync("server").unwrap();
    let mut older = task("older", 1, "older", 1);
    let mut newer = task("newer", 1, "newer", 1);
    older["created_at"] = json!("2026-01-01T12:00:00+10:00");
    newer["created_at"] = json!("2026-01-01T03:00:00Z");
    storage
        .apply_pull(
            "server",
            None,
            "1",
            &[
                ("task/older".into(), older.to_string()),
                ("task/newer".into(), newer.to_string()),
            ],
        )
        .unwrap();
    let loaded = storage.load().unwrap();
    assert_eq!(loaded.tasks["task/older"].task_number, 1);
    assert_eq!(loaded.tasks["task/newer"].task_number, 2);
}

#[derive(Default)]
struct Server {
    sequence: i64,
    values: std::collections::HashMap<String, (i64, Vec<u8>)>,
}

impl Server {
    fn write(&mut self, key: &str, blob: Vec<u8>) {
        self.sequence += 1;
        self.values.insert(key.into(), (self.sequence, blob));
    }

    fn plaintext(&self) -> serde_json::Map<String, Value> {
        self.values
            .iter()
            .map(|(key_name, (_, blob))| {
                let plaintext = saku_crypto::decrypt_entry(blob, key()).unwrap();
                (
                    key_name.clone(),
                    serde_json::from_slice(&plaintext).unwrap(),
                )
            })
            .collect()
    }
}

struct ServerTransport {
    server: std::rc::Rc<std::cell::RefCell<Server>>,
    page_size: usize,
    before_push: Option<(String, Vec<u8>)>,
}

impl Transport for ServerTransport {
    fn pull(&mut self, cookie: Option<&str>) -> Result<PullPage, SyncError> {
        let after = cookie.unwrap_or("0").parse::<i64>().unwrap();
        let server = self.server.borrow();
        let mut rows: Vec<_> = server
            .values
            .iter()
            .filter(|(_, (seq, _))| *seq > after)
            .collect();
        rows.sort_by_key(|(_, (seq, _))| *seq);
        let has_more = rows.len() > self.page_size;
        rows.truncate(self.page_size);
        let next = rows.last().map(|(_, (seq, _))| *seq).unwrap_or(after);
        Ok(PullPage {
            entries: rows
                .into_iter()
                .map(|(key, (_, blob))| RemoteEntry {
                    key: key.clone(),
                    blob: blob.clone(),
                    deleted: false,
                })
                .collect(),
            cookie: next.to_string(),
            has_more,
        })
    }

    fn push(&mut self, entries: &[(String, Vec<u8>)], base_cookie: &str) -> Result<Vec<String>, SyncError> {
        let mut server = self.server.borrow_mut();
        if let Some((key, blob)) = self.before_push.take() {
            server.write(&key, blob);
        }
        let base = base_cookie.parse::<i64>().unwrap();
        if entries.iter().any(|(key, _)| server.values.get(key).is_some_and(|(seq, _)| *seq > base)) {
            return Err(SyncError::StaleWrite);
        }
        for (key, blob) in entries {
            server.write(key, blob.clone());
        }
        Ok(entries.iter().map(|(key, _)| key.clone()).collect())
    }
}

fn assert_converges(
    device_count: usize,
    tasks_per_device: u64,
    page_size: usize,
    area_count: usize,
    reverse: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    let server = std::rc::Rc::new(std::cell::RefCell::new(Server::default()));
    let mut transport = ServerTransport {
        server: server.clone(),
        page_size,
        before_push: None,
    };
    let mut devices = Vec::new();
    let mut originals = serde_json::Map::new();
    for device in 0..device_count {
        let mut storage = SqliteStorage::open(dir.path().join(format!("device-{device}"))).unwrap();
        let mut entries = serde_json::Map::new();
        for number in 1..=tasks_per_device {
            let suffix = format!("d{device}-t{}", tasks_per_device + 1 - number);
            let key_name = format!("task/{suffix}");
            let mut value = task(
                &suffix,
                number,
                &format!("title {device}-{number}"),
                1_767_225_600_000 + device as i64 * 1000,
            );
            value["created_at"] = json!(format!("2026-01-0{}T00:00:00Z", device + 1));
            if device == 0 && number == 1 {
                value["deleted_at"] = json!("2026-02-01T00:00:00Z");
            }
            originals.insert(key_name.clone(), value.clone());
            entries.insert(key_name, value);
        }
        if device == device_count - 1 {
            for index in 0..area_count {
                let name = format!("f{index:03}");
                let value = serde_json::to_value(Area {
                    name: name.clone(),
                    ..Area::default()
                })
                .unwrap();
                originals.insert(format!("area/{name}"), value.clone());
                entries.insert(format!("area/{name}"), value);
            }
        }
        storage
            .import_json(&json!({"version":9,"entries":entries}).to_string())
            .unwrap();
        devices.push(storage);
    }
    let order: Vec<_> = if reverse {
        (0..device_count).rev().collect()
    } else {
        (0..device_count).collect()
    };
    let mut unchanged_rounds = 0;
    for _ in 0..20 {
        let before = server.borrow().sequence;
        for &index in &order {
            sync_replica(&mut devices[index], &mut transport, key(), "server").unwrap();
        }
        if server.borrow().sequence == before {
            unchanged_rounds += 1;
        } else {
            unchanged_rounds = 0;
        }
        if unchanged_rounds == 3 {
            break;
        }
    }
    assert_eq!(
        unchanged_rounds, 3,
        "devices={device_count}, tasks={tasks_per_device}, page={page_size}, reverse={reverse}"
    );
    let remote = server.borrow().plaintext();
    assert_eq!(remote.len(), originals.len());
    for (key, original) in &originals {
        let mut actual = remote[key].clone();
        let mut expected = original.clone();
        if key.starts_with("task/") {
            actual.as_object_mut().unwrap().remove("task_number");
            expected.as_object_mut().unwrap().remove("task_number");
        }
        assert_eq!(actual, expected, "{key}");
    }
    if device_count * tasks_per_device as usize >= 10 {
        assert!(
            remote.iter().any(|(key, value)| key.starts_with("task/")
                && value["task_number"].as_u64().unwrap() >= 10)
        );
    }
    for device in &devices {
        assert!(device.pending_changes().unwrap().is_empty());
        let exported: Value = serde_json::from_str(&device.export_json().unwrap()).unwrap();
        assert_eq!(exported["entries"], Value::Object(remote.clone()));
    }
}

#[test]
fn offline_multi_task_pagination_converges() {
    for page_size in [1, 2, 100] {
        for reverse in [false, true] {
            assert_converges(2, 2, page_size, 0, reverse);
            assert_converges(3, 4, page_size, 0, reverse);
        }
    }
    for reverse in [false, true] {
        assert_converges(2, 2, 100, 99, reverse);
        assert_converges(3, 4, 100, 99, reverse);
    }
}

#[test]
fn remote_update_between_pull_and_push_is_not_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    add(&path, "a", "local");
    let mut storage = SqliteStorage::open(path).unwrap();
    let server = std::rc::Rc::new(std::cell::RefCell::new(Server::default()));
    let area = serde_json::to_vec(&Area {
        name: "remote".into(),
        ..Area::default()
    })
    .unwrap();
    let mut transport = ServerTransport {
        server,
        page_size: 100,
        before_push: Some(("area/remote".into(), encrypt_entry(&area, key()))),
    };
    sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
    assert!(!storage.load().unwrap().areas.contains_key("area/remote"));
    sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
    assert!(storage.load().unwrap().areas.contains_key("area/remote"));
}

#[test]
fn same_key_write_repulls_merges_and_preserves_history_in_both_directions() {
    for (local_ms, expected) in [(300, "local edit"), (150, "remote edit")] {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("db")).unwrap();
        let initial = task("a", 1, "initial", 100);
        storage.import_json(&json!({"version":9,"entries":{"task/a":initial}}).to_string()).unwrap();
        let server = std::rc::Rc::new(std::cell::RefCell::new(Server::default()));
        let mut transport = ServerTransport { server: server.clone(), page_size: 100, before_push: None };
        sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
        sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
        storage.transaction::<_, StorageError>(|store, _| {
            let task = store.tasks.get_mut("task/a").unwrap();
            task.title = "local edit".into();
            task.modified_at.wall_ms = local_ms;
            Ok(())
        }).unwrap();
        let local: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let mut remote = initial.clone();
        remote["title"] = json!("remote edit");
        remote["modified_at"]["wall_ms"] = json!(200);
        transport.before_push = Some(("task/a".into(), encrypt_entry(&serde_json::to_vec(&remote).unwrap(), key())));
        sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, expected);
        assert_eq!(server.borrow().plaintext()["task/a"]["title"], expected);
        let history = storage.history_entries(Some("task/a")).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|entry| entry.source == "local" && entry.value == local["entries"]["task/a"]));
        assert!(history.iter().any(|entry| entry.source == "remote" && entry.value == remote));
        assert!(storage.pending_changes().unwrap().is_empty());
        sync_replica(&mut storage, &mut transport, key(), "server").unwrap();
        assert_eq!(storage.load().unwrap().tasks["task/a"].title, expected);
        assert_eq!(storage.history_entries(Some("task/a")).unwrap(), history);
        assert!(storage.pending_changes().unwrap().is_empty());
    }
}
