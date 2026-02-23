use saku_sync::backend::local_fs::LocalFsSyncBackend;
use saku_sync::{SyncConfig, SyncEngine, SyncOutcome, TrackedFile};
use serde_json::json;
use std::path::PathBuf;

fn make_config(store_path: PathBuf, db_path: PathBuf) -> SyncConfig {
    SyncConfig {
        db_path,
        passphrase: b"integration-test-passphrase".to_vec(),
        tracked_files: vec![TrackedFile {
            file_key: "tdo/store.json".to_string(),
            tool: "tdo".to_string(),
            relative_path: "store.json".to_string(),
            local_path: store_path,
        }],
    }
}

fn write_store(path: &PathBuf, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

/// Two-device simulation:
/// Device A syncs tasks -> Device B pulls -> Device B adds task -> Device A pulls merged result
#[test]
fn two_device_simulation() {
    let remote_dir = tempfile::tempdir().unwrap();
    let device_a_dir = tempfile::tempdir().unwrap();
    let device_b_dir = tempfile::tempdir().unwrap();

    // Device A: create store with one task
    let store_a_path = device_a_dir.path().join("store.json");
    let store_a = json!({
        "version": 4,
        "next_task_number": 2,
        "tasks": [{
            "id": "aaaa-1111",
            "task_number": 1,
            "title": "Task from A",
            "notes": null,
            "project_id": null,
            "area_id": null,
            "tags": [],
            "when": {"type": "Inbox"},
            "deadline": null,
            "defer_until": null,
            "checklist": [],
            "completed_at": null,
            "deleted_at": null,
            "created_at": "2026-01-01T00:00:00Z",
            "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
        }],
        "projects": [],
        "areas": []
    });
    write_store(&store_a_path, &store_a);

    // Device A syncs (push)
    let backend_a = LocalFsSyncBackend::new(remote_dir.path());
    let config_a = make_config(store_a_path.clone(), device_a_dir.path().join("sync.db"));
    let mut engine_a = SyncEngine::new_in_memory(config_a, backend_a).unwrap();
    let outcome = engine_a.sync().unwrap();
    assert!(matches!(outcome, SyncOutcome::Completed { pushed: 1, .. }));

    // Device B: create empty store
    let store_b_path = device_b_dir.path().join("store.json");
    let store_b = json!({
        "version": 4,
        "next_task_number": 1,
        "tasks": [],
        "projects": [],
        "areas": []
    });
    write_store(&store_b_path, &store_b);

    // Device B syncs (pull)
    let backend_b = LocalFsSyncBackend::new(remote_dir.path());
    let config_b = make_config(store_b_path.clone(), device_b_dir.path().join("sync.db"));
    let mut engine_b = SyncEngine::new_in_memory(config_b, backend_b).unwrap();
    let outcome_b = engine_b.sync().unwrap();
    match outcome_b {
        SyncOutcome::Completed { pulled, .. } => {
            assert!(pulled > 0, "Device B should have pulled changes");
        }
        _ => panic!("Expected Completed"),
    }

    // Verify Device B now has the task from A
    let b_data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store_b_path).unwrap()).unwrap();
    let tasks = b_data["tasks"].as_array().unwrap();
    assert!(!tasks.is_empty(), "Device B should have tasks after pull");

    // Device B adds a new task
    let store_b_updated = json!({
        "version": 4,
        "next_task_number": 3,
        "tasks": [
            {
                "id": "aaaa-1111",
                "task_number": 1,
                "title": "Task from A",
                "notes": null,
                "project_id": null,
                "area_id": null,
                "tags": [],
                "when": {"type": "Inbox"},
                "deadline": null,
                "defer_until": null,
                "checklist": [],
                "completed_at": null,
                "deleted_at": null,
                "created_at": "2026-01-01T00:00:00Z",
                "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
            },
            {
                "id": "bbbb-2222",
                "task_number": 2,
                "title": "Task from B",
                "notes": null,
                "project_id": null,
                "area_id": null,
                "tags": [],
                "when": {"type": "Inbox"},
                "deadline": null,
                "defer_until": null,
                "checklist": [],
                "completed_at": null,
                "deleted_at": null,
                "created_at": "2026-01-02T00:00:00Z",
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-b"}
            }
        ],
        "projects": [],
        "areas": []
    });
    write_store(&store_b_path, &store_b_updated);

    // Device B syncs (push new task)
    let backend_b2 = LocalFsSyncBackend::new(remote_dir.path());
    let config_b2 = make_config(store_b_path.clone(), device_b_dir.path().join("sync2.db"));
    let mut engine_b2 = SyncEngine::new_in_memory(config_b2, backend_b2).unwrap();
    engine_b2.sync().unwrap();

    // Device A syncs (pull B's changes)
    let backend_a2 = LocalFsSyncBackend::new(remote_dir.path());
    let config_a2 = make_config(store_a_path.clone(), device_a_dir.path().join("sync2.db"));
    let mut engine_a2 = SyncEngine::new_in_memory(config_a2, backend_a2).unwrap();
    engine_a2.sync().unwrap();

    // Verify Device A now has both tasks
    let a_data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store_a_path).unwrap()).unwrap();
    let a_tasks = a_data["tasks"].as_array().unwrap();
    assert_eq!(a_tasks.len(), 2, "Device A should have 2 tasks after merge");
}

/// LWW conflict: Same entity edited on both sides, later timestamp wins
#[test]
fn lww_conflict_later_timestamp_wins() {
    let remote_dir = tempfile::tempdir().unwrap();
    let device_a_dir = tempfile::tempdir().unwrap();
    let device_b_dir = tempfile::tempdir().unwrap();

    let base_task = json!({
        "id": "conflict-task-1",
        "task_number": 1,
        "title": "Original title",
        "notes": null,
        "project_id": null,
        "area_id": null,
        "tags": [],
        "when": {"type": "Inbox"},
        "deadline": null,
        "defer_until": null,
        "checklist": [],
        "completed_at": null,
        "deleted_at": null,
        "created_at": "2026-01-01T00:00:00Z",
        "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
    });

    // Device A: edit task with earlier timestamp
    let store_a_path = device_a_dir.path().join("store.json");
    let mut task_a = base_task.clone();
    task_a["title"] = json!("Title from A");
    task_a["modified_at"] = json!({"wall_ms": 2000, "lamport": 1, "device_id": "dev-a"});
    write_store(
        &store_a_path,
        &json!({
            "version": 4, "next_task_number": 2,
            "tasks": [task_a], "projects": [], "areas": []
        }),
    );

    // Device A syncs first
    let backend_a = LocalFsSyncBackend::new(remote_dir.path());
    let config_a = make_config(store_a_path.clone(), device_a_dir.path().join("sync.db"));
    let mut engine_a = SyncEngine::new_in_memory(config_a, backend_a).unwrap();
    engine_a.sync().unwrap();

    // Device B: edit same task with LATER timestamp
    let store_b_path = device_b_dir.path().join("store.json");
    let mut task_b = base_task.clone();
    task_b["title"] = json!("Title from B (winner)");
    task_b["modified_at"] = json!({"wall_ms": 3000, "lamport": 1, "device_id": "dev-b"});
    write_store(
        &store_b_path,
        &json!({
            "version": 4, "next_task_number": 2,
            "tasks": [task_b], "projects": [], "areas": []
        }),
    );

    // Device B syncs (push + pull merge)
    let backend_b = LocalFsSyncBackend::new(remote_dir.path());
    let config_b = make_config(store_b_path.clone(), device_b_dir.path().join("sync.db"));
    let mut engine_b = SyncEngine::new_in_memory(config_b, backend_b).unwrap();
    engine_b.sync().unwrap();

    // Verify Device B's store has the winning title
    let b_data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store_b_path).unwrap()).unwrap();
    let tasks = b_data["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Title from B (winner)");
}

/// Note conflict: Both sides edit same non-JSON file, conflict file created
#[test]
fn note_conflict_creates_conflict_file() {
    let remote_dir = tempfile::tempdir().unwrap();
    let device_a_dir = tempfile::tempdir().unwrap();
    let device_b_dir = tempfile::tempdir().unwrap();

    // Device A: create a note file
    let note_a_path = device_a_dir.path().join("note.md");
    std::fs::write(&note_a_path, b"Note from device A").unwrap();

    let config_a = SyncConfig {
        db_path: device_a_dir.path().join("sync.db"),
        passphrase: b"test-pass".to_vec(),
        tracked_files: vec![TrackedFile {
            file_key: "nte/note.md".to_string(),
            tool: "nte".to_string(),
            relative_path: "note.md".to_string(),
            local_path: note_a_path.clone(),
        }],
    };

    let backend_a = LocalFsSyncBackend::new(remote_dir.path());
    let mut engine_a = SyncEngine::new_in_memory(config_a, backend_a).unwrap();
    engine_a.sync().unwrap();

    // Device B: create same note with different content
    let note_b_path = device_b_dir.path().join("note.md");
    std::fs::write(&note_b_path, b"Note from device B").unwrap();

    let config_b = SyncConfig {
        db_path: device_b_dir.path().join("sync.db"),
        passphrase: b"test-pass".to_vec(),
        tracked_files: vec![TrackedFile {
            file_key: "nte/note.md".to_string(),
            tool: "nte".to_string(),
            relative_path: "note.md".to_string(),
            local_path: note_b_path.clone(),
        }],
    };

    let backend_b = LocalFsSyncBackend::new(remote_dir.path());
    let mut engine_b = SyncEngine::new_in_memory(config_b, backend_b).unwrap();
    engine_b.sync().unwrap();

    // Device B should still have its local version, plus a conflict file
    let local_content = std::fs::read_to_string(&note_b_path).unwrap();
    assert_eq!(local_content, "Note from device B");

    // Check for conflict file
    let entries: Vec<_> = std::fs::read_dir(device_b_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|s| s.contains("conflict"))
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !entries.is_empty(),
        "Should have created a conflict file for the note"
    );
}

/// Three devices each create task #1 offline, then all sync.
/// The final merged store must have no duplicate task_numbers.
#[test]
fn three_device_offline_no_duplicates() {
    let remote_dir = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let dir_c = tempfile::tempdir().unwrap();

    let make_store = |device_id: &str, task_id: &str, created_at: &str, wall_ms: i64| {
        json!({
            "version": 7,
            "next_task_number": 2,
            "tasks": [{
                "id": task_id,
                "task_number": 1,
                "title": format!("Task from {}", device_id),
                "notes": null,
                "project_id": null,
                "area_id": null,
                "parent_task_id": null,
                "tags": [],
                "when": {"type": "Inbox"},
                "deadline": null,
                "defer_until": null,
                "completed_at": null,
                "deleted_at": null,
                "depends_on": [],
                "created_at": created_at,
                "modified_at": {"wall_ms": wall_ms, "lamport": 1, "device_id": device_id}
            }],
            "projects": [],
            "areas": []
        })
    };

    let store_a_path = dir_a.path().join("store.json");
    let store_b_path = dir_b.path().join("store.json");
    let store_c_path = dir_c.path().join("store.json");

    write_store(&store_a_path, &make_store("dev-a", "task-aaa", "2026-01-01T00:00:00Z", 1000));
    write_store(&store_b_path, &make_store("dev-b", "task-bbb", "2026-01-02T00:00:00Z", 2000));
    write_store(&store_c_path, &make_store("dev-c", "task-ccc", "2026-01-03T00:00:00Z", 3000));

    // Device A syncs first
    let mut engine_a = SyncEngine::new_in_memory(
        make_config(store_a_path.clone(), dir_a.path().join("sync.db")),
        LocalFsSyncBackend::new(remote_dir.path()),
    ).unwrap();
    engine_a.sync().unwrap();

    // Device B syncs
    let mut engine_b = SyncEngine::new_in_memory(
        make_config(store_b_path.clone(), dir_b.path().join("sync.db")),
        LocalFsSyncBackend::new(remote_dir.path()),
    ).unwrap();
    engine_b.sync().unwrap();

    // Device C syncs
    let mut engine_c = SyncEngine::new_in_memory(
        make_config(store_c_path.clone(), dir_c.path().join("sync.db")),
        LocalFsSyncBackend::new(remote_dir.path()),
    ).unwrap();
    engine_c.sync().unwrap();

    // Device A syncs again to pull B and C
    let mut engine_a2 = SyncEngine::new_in_memory(
        make_config(store_a_path.clone(), dir_a.path().join("sync2.db")),
        LocalFsSyncBackend::new(remote_dir.path()),
    ).unwrap();
    engine_a2.sync().unwrap();

    let final_store: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store_a_path).unwrap()).unwrap();

    let tasks = final_store["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3, "All 3 tasks must be present");

    let mut numbers: Vec<u64> = tasks
        .iter()
        .filter_map(|t| t["task_number"].as_u64())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    assert_eq!(numbers.len(), 3, "All task numbers must be unique: {:?}", numbers);
}

/// Offline queue: Backend unreachable -> ops queue -> backend comes back -> flush succeeds
#[test]
fn offline_queue_flushes_when_online() {
    let local_dir = tempfile::tempdir().unwrap();
    let remote_dir = tempfile::tempdir().unwrap();

    let store_path = local_dir.path().join("store.json");
    write_store(
        &store_path,
        &json!({
            "version": 4, "next_task_number": 2,
            "tasks": [{"id": "t1", "title": "test", "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "a"}}],
            "projects": [], "areas": []
        }),
    );

    // First try: backend unreachable
    let backend = LocalFsSyncBackend::new(std::path::Path::new("/nonexistent/remote"));
    let config = make_config(store_path.clone(), local_dir.path().join("sync.db"));
    let mut engine = SyncEngine::new_in_memory(config, backend).unwrap();
    let outcome = engine.sync().unwrap();
    assert!(matches!(outcome, SyncOutcome::Skipped));

    // Second try: backend is now available
    let backend2 = LocalFsSyncBackend::new(remote_dir.path());
    let config2 = make_config(store_path.clone(), local_dir.path().join("sync2.db"));
    let mut engine2 = SyncEngine::new_in_memory(config2, backend2).unwrap();
    let outcome2 = engine2.sync().unwrap();
    match outcome2 {
        SyncOutcome::Completed { pushed, .. } => {
            assert!(pushed > 0, "Should flush pending uploads");
        }
        _ => panic!("Expected Completed after backend becomes available"),
    }

    // Verify file exists on remote
    assert!(remote_dir.path().join("tdo/store.json.enc").exists());
}
