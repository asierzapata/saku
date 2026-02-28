use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::SyncError;
use saku_storage::entity::EntitySchema;
use saku_storage::kv_store::{self, KvStore};

/// Entity schemas for the tdo tool — describes which fields are
/// cross-entity foreign key references for generic repair.
fn tdo_entity_schemas() -> Vec<EntitySchema> {
    vec![
        EntitySchema {
            entity_type: "task",
            references: vec![
                ("project_key", "project"),
                ("area_key", "area"),
                ("parent_task_key", "task"),
                ("depends_on", "task"),
            ],
        },
        EntitySchema {
            entity_type: "project",
            references: vec![("area_key", "area")],
        },
    ]
}

/// Fix duplicate task_numbers in KV entries.
///
/// When multiple devices create tasks offline, they may both assign the same
/// task_number. The oldest task (by created_at, then key) keeps its number;
/// duplicates get new numbers starting from max(existing_numbers) + 1.
fn fix_duplicate_task_numbers_kv(entries: &mut HashMap<String, Value>) {
    // Collect task entries and their task_numbers
    let task_entries: Vec<(String, u64)> = entries
        .iter()
        .filter(|(k, _)| k.starts_with("task/"))
        .filter_map(|(k, v)| {
            v.get("task_number")
                .and_then(|n| n.as_u64())
                .map(|n| (k.clone(), n))
        })
        .collect();

    // Group by task_number to find duplicates
    let mut by_number: HashMap<u64, Vec<String>> = HashMap::new();
    for (key, number) in &task_entries {
        by_number.entry(*number).or_default().push(key.clone());
    }

    let max_number = task_entries.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let mut next = max_number + 1;

    for keys in by_number.values().filter(|v| v.len() > 1) {
        // Sort by created_at, then key for stability
        let mut sorted = keys.clone();
        sorted.sort_by(|a, b| {
            let ca = entries[a]
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cb = entries[b]
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            ca.cmp(cb).then(a.cmp(b))
        });

        // First keeps its number, rest get new numbers
        for key in &sorted[1..] {
            if let Some(entry) = entries.get_mut(key) {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("task_number".to_string(), Value::from(next));
                    next += 1;
                }
            }
        }
    }
}

/// Merge two v9 KV store JSON values.
///
/// 1. Deserializes both as `KvStore` (v9 flat entries format)
/// 2. Calls `lww_merge_kv` for per-entry last-writer-wins merge
/// 3. Calls `reconcile_renames` to resolve tombstone-based renames
/// 4. Calls `repair_references` to fix dangling FK references
/// 5. Deduplicates task_numbers from parallel offline creation
/// 6. Serializes the merged result back to `Value`
pub fn merge_store_json(local: &Value, remote: &Value) -> Value {
    let local_kv: KvStore = serde_json::from_value(local.clone())
        .unwrap_or_else(|_| KvStore::new(9));
    let remote_kv: KvStore = serde_json::from_value(remote.clone())
        .unwrap_or_else(|_| KvStore::new(9));

    let mut merged = kv_store::lww_merge_kv(&local_kv, &remote_kv);

    kv_store::reconcile_renames(&mut merged);
    kv_store::repair_references(&mut merged, &tdo_entity_schemas());
    fix_duplicate_task_numbers_kv(&mut merged.entries);

    serde_json::to_value(&merged).unwrap_or_else(|_| local.clone())
}

/// For non-JSON files (e.g. notes), write the remote version as a conflict copy.
/// Returns the path of the conflict file.
pub fn write_conflict_copy(
    path: &Path,
    remote_content: &[u8],
    device_id: &str,
) -> Result<PathBuf, SyncError> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    let conflict_name = if ext.is_empty() {
        format!("{}.{}.conflict", stem, device_id)
    } else {
        format!("{}.{}.conflict.{}", stem, device_id, ext)
    };

    let conflict_path = path.with_file_name(conflict_name);
    std::fs::write(&conflict_path, remote_content)?;
    Ok(conflict_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_kv_store(entries: Vec<(&str, Value)>) -> Value {
        let mut map = serde_json::Map::new();
        for (key, value) in entries {
            map.insert(key.to_string(), value);
        }
        json!({
            "version": 9,
            "entries": Value::Object(map)
        })
    }

    fn make_task(title: &str, task_number: u64, wall_ms: i64, device_id: &str) -> Value {
        json!({
            "storage_key_suffix": format!("task-{}", task_number),
            "task_number": task_number,
            "title": title,
            "notes": null,
            "project_key": null,
            "area_key": null,
            "parent_task_key": null,
            "depends_on": [],
            "tags": [],
            "when": {"type": "Inbox"},
            "deadline": null,
            "defer_until": null,
            "completed_at": null,
            "deleted_at": null,
            "created_at": "2026-01-01T00:00:00Z",
            "modified_at": {"wall_ms": wall_ms, "lamport": 1, "device_id": device_id}
        })
    }

    fn make_task_with_created_at(
        title: &str,
        task_number: u64,
        wall_ms: i64,
        device_id: &str,
        created_at: &str,
    ) -> Value {
        let mut task = make_task(title, task_number, wall_ms, device_id);
        task["created_at"] = json!(created_at);
        task
    }

    fn make_project(name: &str, wall_ms: i64, device_id: &str) -> Value {
        json!({
            "name": name,
            "area_key": null,
            "notes": null,
            "deadline": null,
            "completed_at": null,
            "deleted_at": null,
            "created_at": "2026-01-01T00:00:00Z",
            "modified_at": {"wall_ms": wall_ms, "lamport": 1, "device_id": device_id}
        })
    }

    fn make_area(name: &str, wall_ms: i64, device_id: &str) -> Value {
        json!({
            "name": name,
            "deleted_at": null,
            "modified_at": {"wall_ms": wall_ms, "lamport": 1, "device_id": device_id}
        })
    }

    #[test]
    fn lww_winner_selection() {
        let local = make_kv_store(vec![(
            "task/t1",
            json!({
                "title": "local version",
                "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
            }),
        )]);
        let remote = make_kv_store(vec![(
            "task/t1",
            json!({
                "title": "remote version",
                "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
            }),
        )]);

        let merged = merge_store_json(&local, &remote);
        assert_eq!(merged["entries"]["task/t1"]["title"], "remote version");
    }

    #[test]
    fn lww_local_wins_when_newer() {
        let local = make_kv_store(vec![(
            "task/t1",
            json!({
                "title": "local version",
                "modified_at": {"wall_ms": 300, "lamport": 1, "device_id": "dev-a"}
            }),
        )]);
        let remote = make_kv_store(vec![(
            "task/t1",
            json!({
                "title": "remote version",
                "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
            }),
        )]);

        let merged = merge_store_json(&local, &remote);
        assert_eq!(merged["entries"]["task/t1"]["title"], "local version");
    }

    #[test]
    fn entity_only_in_one_side_included() {
        let local = make_kv_store(vec![(
            "task/t1",
            json!({
                "title": "local only",
                "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
            }),
        )]);
        let remote = make_kv_store(vec![(
            "task/t2",
            json!({
                "title": "remote only",
                "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-b"}
            }),
        )]);

        let merged = merge_store_json(&local, &remote);
        let entries = merged["entries"].as_object().unwrap();
        assert!(entries.contains_key("task/t1"));
        assert!(entries.contains_key("task/t2"));
    }

    #[test]
    fn merge_store_includes_all_entries() {
        let local = make_kv_store(vec![("task/t1", make_task("task1", 1, 100, "a"))]);
        let remote = make_kv_store(vec![("task/t2", make_task("task2", 2, 200, "b"))]);

        let merged = merge_store_json(&local, &remote);
        let entries = merged["entries"].as_object().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn merge_deduplicates_task_numbers() {
        let local = make_kv_store(vec![(
            "task/local-1",
            make_task_with_created_at("Local task", 1, 1000, "dev-a", "2026-01-01T00:00:00Z"),
        )]);
        let remote = make_kv_store(vec![(
            "task/remote-1",
            make_task_with_created_at("Remote task", 1, 2000, "dev-b", "2026-01-02T00:00:00Z"),
        )]);

        let merged = merge_store_json(&local, &remote);
        let entries = merged["entries"].as_object().unwrap();

        // Both tasks present
        assert!(entries.contains_key("task/local-1"));
        assert!(entries.contains_key("task/remote-1"));

        // Unique task numbers
        let n1 = entries["task/local-1"]["task_number"].as_u64().unwrap();
        let n2 = entries["task/remote-1"]["task_number"].as_u64().unwrap();
        assert_ne!(n1, n2, "Task numbers must be unique after merge");

        // Older task keeps number 1
        assert_eq!(n1, 1, "Older task (Local) keeps its number");
    }

    #[test]
    fn merge_with_project_and_area() {
        let local = make_kv_store(vec![
            ("area/work", make_area("Work", 1000, "dev-a")),
            ("project/website", make_project("Website", 1000, "dev-a")),
            ("task/t1", {
                let mut t = make_task("Build landing page", 1, 1000, "dev-a");
                t["project_key"] = json!("project/website");
                t["area_key"] = json!("area/work");
                t
            }),
        ]);
        let remote = make_kv_store(vec![
            ("area/work", make_area("Work", 2000, "dev-b")),
            ("task/t2", make_task("Deploy app", 2, 2000, "dev-b")),
        ]);

        let merged = merge_store_json(&local, &remote);
        let entries = merged["entries"].as_object().unwrap();

        // Area from remote wins (newer)
        assert_eq!(entries["area/work"]["modified_at"]["wall_ms"], 2000);
        // Project preserved from local
        assert!(entries.contains_key("project/website"));
        // Both tasks present
        assert!(entries.contains_key("task/t1"));
        assert!(entries.contains_key("task/t2"));
        // Task references intact
        assert_eq!(entries["task/t1"]["project_key"], "project/website");
        assert_eq!(entries["task/t1"]["area_key"], "area/work");
    }

    #[test]
    fn merge_repairs_references_after_rename() {
        // Local: project was renamed from "website" to "blog" with tombstone
        let local = make_kv_store(vec![
            ("project/website", json!({
                "name": "Website",
                "deleted_at": "2026-01-15T00:00:00Z",
                "renamed_to": "project/blog",
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-a"}
            })),
            ("project/blog", json!({
                "name": "Blog",
                "area_key": null,
                "notes": null, "deadline": null,
                "completed_at": null, "deleted_at": null,
                "created_at": "2026-01-01T00:00:00Z",
                "previous_key": "project/website",
                "modified_at": {"wall_ms": 2000, "lamport": 2, "device_id": "dev-a"}
            })),
        ]);

        // Remote: still has tasks pointing to old project key
        let remote = make_kv_store(vec![
            ("task/t1", {
                let mut t = make_task("Fix bug", 1, 1500, "dev-b");
                t["project_key"] = json!("project/website");
                t
            }),
        ]);

        let merged = merge_store_json(&local, &remote);
        let entries = merged["entries"].as_object().unwrap();

        // Task's project_key should be repaired to point to the new name
        assert_eq!(
            entries["task/t1"]["project_key"], "project/blog",
            "Reference should be repaired after rename"
        );
    }

    #[test]
    fn conflict_file_creation() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.md");
        std::fs::write(&file_path, b"local content").unwrap();

        let conflict_path = write_conflict_copy(&file_path, b"remote content", "dev-b").unwrap();

        assert!(conflict_path.exists());
        assert!(conflict_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("dev-b"));
        assert_eq!(
            std::fs::read_to_string(&conflict_path).unwrap(),
            "remote content"
        );
    }
}
