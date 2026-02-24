use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::SyncError;

/// Merge two JSON arrays of entities by `id` field using Last-Writer-Wins (LWW).
///
/// For entities present in both arrays, the one with the greater `modified_at`
/// (compared as HybridTimestamp: wall_ms → lamport → device_id) wins.
/// Entities only in one side are always included.
pub fn lww_merge_entity_array(local: &[Value], remote: &[Value]) -> Vec<Value> {
    use std::collections::HashMap;

    // Index local entities by id
    let mut merged: HashMap<String, &Value> = HashMap::new();
    for entity in local {
        if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
            merged.insert(id.to_string(), entity);
        }
    }

    // Merge remote entities
    for entity in remote {
        if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
            let id_str = id.to_string();
            match merged.get(&id_str) {
                Some(local_entity) => {
                    if compare_modified_at(entity, local_entity) == std::cmp::Ordering::Greater {
                        merged.insert(id_str, entity);
                    }
                }
                None => {
                    merged.insert(id_str, entity);
                }
            }
        }
    }

    merged.into_values().cloned().collect()
}

/// Compare `modified_at` fields of two JSON entities.
/// Returns Ordering of `a` relative to `b`.
fn compare_modified_at(a: &Value, b: &Value) -> std::cmp::Ordering {
    let a_ts = extract_modified_at(a);
    let b_ts = extract_modified_at(b);
    a_ts.cmp(&b_ts)
}

/// Extract (wall_ms, lamport, device_id) from a JSON entity's `modified_at` field.
fn extract_modified_at(entity: &Value) -> (i64, u64, String) {
    let ma = entity.get("modified_at");
    let wall_ms = ma
        .and_then(|v| v.get("wall_ms"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let lamport = ma
        .and_then(|v| v.get("lamport"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let device_id = ma
        .and_then(|v| v.get("device_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (wall_ms, lamport, device_id)
}

/// Reassign task_numbers to any tasks that share a number with another task.
/// Oldest task (by created_at, then id) keeps its number; duplicates get new numbers
/// starting from max(existing_numbers) + 1.
/// Returns the new next_task_number ceiling.
fn fix_duplicate_task_numbers(tasks: &mut [Value]) -> u64 {
    use std::collections::HashMap;

    let mut by_number: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, task) in tasks.iter().enumerate() {
        if let Some(n) = task.get("task_number").and_then(|v| v.as_u64()) {
            by_number.entry(n).or_default().push(i);
        }
    }

    let max_number = by_number.keys().max().copied().unwrap_or(0);
    let mut next = max_number + 1;

    for indices in by_number.values().filter(|v| v.len() > 1) {
        let mut sorted = indices.clone();
        sorted.sort_by(|&a, &b| {
            let ca = tasks[a].get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let cb = tasks[b].get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let ia = tasks[a].get("id").and_then(|v| v.as_str()).unwrap_or("");
            let ib = tasks[b].get("id").and_then(|v| v.as_str()).unwrap_or("");
            ca.cmp(cb).then(ia.cmp(ib))
        });
        // First entry keeps its number; reassign the rest
        for &idx in &sorted[1..] {
            if let Some(obj) = tasks[idx].as_object_mut() {
                obj.insert("task_number".to_string(), Value::from(next));
                next += 1;
            }
        }
    }

    next
}

/// Detect duplicate entity names after UUID-based merge and soft-delete the newer duplicates.
/// Entities are grouped by case-insensitive name; already-deleted entities are excluded.
/// The oldest entity (by `created_at` string if present, else `modified_at.wall_ms`, then `id`)
/// keeps its entry; duplicates are soft-deleted with `deleted_at` set to the current timestamp.
fn fix_duplicate_names(entities: &mut [Value]) {
    use std::collections::HashMap;

    let now = jiff::Timestamp::now().to_string();

    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, entity) in entities.iter().enumerate() {
        if entity.get("deleted_at").is_some_and(|v| !v.is_null()) {
            continue;
        }
        if let Some(name) = entity.get("name").and_then(|v| v.as_str()) {
            by_name.entry(name.to_lowercase()).or_default().push(i);
        }
    }

    for indices in by_name.values().filter(|v| v.len() > 1) {
        let mut sorted = indices.clone();
        sorted.sort_by(|&a, &b| {
            let ca = entities[a].get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let cb = entities[b].get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let ma = entities[a]
                .get("modified_at")
                .and_then(|v| v.get("wall_ms"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let mb = entities[b]
                .get("modified_at")
                .and_then(|v| v.get("wall_ms"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let ia = entities[a].get("id").and_then(|v| v.as_str()).unwrap_or("");
            let ib = entities[b].get("id").and_then(|v| v.as_str()).unwrap_or("");
            ca.cmp(cb).then(ma.cmp(&mb)).then(ia.cmp(ib))
        });
        // First (oldest) keeps its entry; soft-delete the rest
        for &idx in &sorted[1..] {
            if let Some(obj) = entities[idx].as_object_mut() {
                obj.insert("deleted_at".to_string(), Value::String(now.clone()));
            }
        }
    }
}

/// Merge two complete store JSON values using LWW for tasks, projects, areas.
/// Takes the maximum of `next_task_number` and deduplicates any task numbers
/// introduced by parallel offline creation on multiple devices.
pub fn lww_merge_store_json(local: &Value, remote: &Value) -> Value {
    let mut result = local.clone();

    // Merge each entity array
    for key in &["tasks", "projects", "areas"] {
        let local_arr = local
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let remote_arr = remote
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let merged = lww_merge_entity_array(&local_arr, &remote_arr);
        result[key] = Value::Array(merged);
    }

    // Fix duplicate task_numbers introduced by parallel offline creation
    let mut tasks_arr = result["tasks"].as_array().cloned().unwrap_or_default();
    let dedupe_next = fix_duplicate_task_numbers(&mut tasks_arr);
    result["tasks"] = Value::Array(tasks_arr);

    // Fix duplicate names for projects and areas introduced by parallel offline creation
    for key in &["projects", "areas"] {
        let mut arr = result[key].as_array().cloned().unwrap_or_default();
        fix_duplicate_names(&mut arr);
        result[*key] = Value::Array(arr);
    }

    // next_task_number = max of both sides AND any new numbers allocated during dedup
    let local_ntn = local
        .get("next_task_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let remote_ntn = remote
        .get("next_task_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    result["next_task_number"] = Value::Number(local_ntn.max(remote_ntn).max(dedupe_next).into());

    result
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

    #[test]
    fn lww_winner_selection() {
        let local = vec![json!({
            "id": "task-1",
            "title": "local version",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
        })];

        let remote = vec![json!({
            "id": "task-1",
            "title": "remote version",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        })];

        let merged = lww_merge_entity_array(&local, &remote);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["title"], "remote version");
    }

    #[test]
    fn lww_local_wins_when_newer() {
        let local = vec![json!({
            "id": "task-1",
            "title": "local version",
            "modified_at": {"wall_ms": 300, "lamport": 1, "device_id": "dev-a"}
        })];

        let remote = vec![json!({
            "id": "task-1",
            "title": "remote version",
            "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
        })];

        let merged = lww_merge_entity_array(&local, &remote);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["title"], "local version");
    }

    #[test]
    fn entity_only_in_one_side_included() {
        let local = vec![json!({
            "id": "task-1",
            "title": "local only",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
        })];

        let remote = vec![json!({
            "id": "task-2",
            "title": "remote only",
            "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-b"}
        })];

        let merged = lww_merge_entity_array(&local, &remote);
        assert_eq!(merged.len(), 2);

        let ids: Vec<&str> = merged
            .iter()
            .filter_map(|v| v.get("id").and_then(|id| id.as_str()))
            .collect();
        assert!(ids.contains(&"task-1"));
        assert!(ids.contains(&"task-2"));
    }

    #[test]
    fn merge_store_json() {
        let local = json!({
            "version": 4,
            "next_task_number": 5,
            "tasks": [
                {"id": "t1", "title": "task1", "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "a"}}
            ],
            "projects": [],
            "areas": []
        });

        let remote = json!({
            "version": 4,
            "next_task_number": 10,
            "tasks": [
                {"id": "t2", "title": "task2", "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "b"}}
            ],
            "projects": [],
            "areas": []
        });

        let merged = lww_merge_store_json(&local, &remote);
        assert_eq!(merged["next_task_number"], 10);

        let tasks = merged["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn merge_deduplicates_task_numbers() {
        let local = json!({
            "version": 7,
            "next_task_number": 2,
            "tasks": [{
                "id": "local-task-1",
                "task_number": 1,
                "title": "Local task",
                "created_at": "2026-01-01T00:00:00Z",
                "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
            }],
            "projects": [],
            "areas": []
        });

        let remote = json!({
            "version": 7,
            "next_task_number": 2,
            "tasks": [{
                "id": "remote-task-1",
                "task_number": 1,
                "title": "Remote task",
                "created_at": "2026-01-02T00:00:00Z",
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-b"}
            }],
            "projects": [],
            "areas": []
        });

        let merged = lww_merge_store_json(&local, &remote);

        let tasks = merged["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2, "Both tasks must be present after merge");

        let mut numbers: Vec<u64> = tasks
            .iter()
            .filter_map(|t| t["task_number"].as_u64())
            .collect();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), 2, "Task numbers must be unique after merge");

        let local_task = tasks.iter().find(|t| t["id"] == "local-task-1").unwrap();
        assert_eq!(local_task["task_number"], 1, "Older task keeps number 1");

        assert!(merged["next_task_number"].as_u64().unwrap() >= 3);
    }

    #[test]
    fn merge_deduplicates_project_names() {
        let local = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [{
                "id": "proj-aaa",
                "name": "Website",
                "area_id": null,
                "notes": null,
                "deadline": null,
                "completed_at": null,
                "deleted_at": null,
                "created_at": "2026-01-01T00:00:00Z",
                "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
            }],
            "areas": []
        });

        let remote = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [{
                "id": "proj-bbb",
                "name": "Website",
                "area_id": null,
                "notes": null,
                "deadline": null,
                "completed_at": null,
                "deleted_at": null,
                "created_at": "2026-01-02T00:00:00Z",
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-b"}
            }],
            "areas": []
        });

        let merged = lww_merge_store_json(&local, &remote);
        let projects = merged["projects"].as_array().unwrap();

        assert_eq!(projects.len(), 2, "Both UUID entries must be present");

        let active: Vec<&Value> = projects
            .iter()
            .filter(|p| p["deleted_at"].is_null())
            .collect();
        assert_eq!(active.len(), 1, "Only one project should be active after dedup");
        assert_eq!(active[0]["id"], "proj-aaa", "Oldest project (by created_at) wins");
    }

    #[test]
    fn merge_deduplicates_area_names() {
        let local = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [],
            "areas": [{
                "id": "area-aaa",
                "name": "Work",
                "deleted_at": null,
                "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
            }]
        });

        let remote = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [],
            "areas": [{
                "id": "area-bbb",
                "name": "Work",
                "deleted_at": null,
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-b"}
            }]
        });

        let merged = lww_merge_store_json(&local, &remote);
        let areas = merged["areas"].as_array().unwrap();

        assert_eq!(areas.len(), 2, "Both UUID entries must be present");

        let active: Vec<&Value> = areas
            .iter()
            .filter(|a| a["deleted_at"].is_null())
            .collect();
        assert_eq!(active.len(), 1, "Only one area should be active after dedup");
        assert_eq!(
            active[0]["id"], "area-aaa",
            "Area with earlier modified_at.wall_ms wins"
        );
    }

    #[test]
    fn merge_deduplicates_project_names_case_insensitive() {
        // "website" and "Website" are the same name; "Blog" is distinct and must survive
        let local = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [
                {
                    "id": "proj-aaa",
                    "name": "Website",
                    "deleted_at": null,
                    "created_at": "2026-01-01T00:00:00Z",
                    "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
                },
                {
                    "id": "proj-ccc",
                    "name": "Blog",
                    "deleted_at": null,
                    "created_at": "2026-01-01T00:00:00Z",
                    "modified_at": {"wall_ms": 1000, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "areas": []
        });

        let remote = json!({
            "version": 7,
            "next_task_number": 1,
            "tasks": [],
            "projects": [{
                "id": "proj-bbb",
                "name": "website",
                "deleted_at": null,
                "created_at": "2026-01-02T00:00:00Z",
                "modified_at": {"wall_ms": 2000, "lamport": 1, "device_id": "dev-b"}
            }],
            "areas": []
        });

        let merged = lww_merge_store_json(&local, &remote);
        let projects = merged["projects"].as_array().unwrap();

        assert_eq!(projects.len(), 3, "All three UUID entries must be present");

        let active: Vec<&Value> = projects
            .iter()
            .filter(|p| p["deleted_at"].is_null())
            .collect();
        assert_eq!(active.len(), 2, "Blog + one website variant should be active");

        let active_ids: Vec<&str> = active
            .iter()
            .filter_map(|p| p["id"].as_str())
            .collect();
        assert!(active_ids.contains(&"proj-aaa"), "Oldest website wins");
        assert!(active_ids.contains(&"proj-ccc"), "Blog is unaffected");
    }

    #[test]
    fn conflict_file_creation() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.md");
        std::fs::write(&file_path, b"local content").unwrap();

        let conflict_path = write_conflict_copy(&file_path, b"remote content", "dev-b").unwrap();

        assert!(conflict_path.exists());
        assert!(
            conflict_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("dev-b")
        );
        assert_eq!(
            std::fs::read_to_string(&conflict_path).unwrap(),
            "remote content"
        );
    }
}
