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

/// Merge two complete store JSON values using LWW for tasks, projects, areas.
/// Takes the maximum of `next_task_number`.
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

    // Take max of next_task_number
    let local_ntn = local
        .get("next_task_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let remote_ntn = remote
        .get("next_task_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    result["next_task_number"] = Value::Number(local_ntn.max(remote_ntn).into());

    result
}

/// For non-JSON files (e.g. notes), write the remote version as a conflict copy.
/// Returns the path of the conflict file.
pub fn write_conflict_copy(
    path: &Path,
    remote_content: &[u8],
    device_id: &str,
) -> Result<PathBuf, SyncError> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

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
    fn conflict_file_creation() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.md");
        std::fs::write(&file_path, b"local content").unwrap();

        let conflict_path =
            write_conflict_copy(&file_path, b"remote content", "dev-b").unwrap();

        assert!(conflict_path.exists());
        assert!(conflict_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("dev-b"));
        assert_eq!(std::fs::read_to_string(&conflict_path).unwrap(), "remote content");
    }
}
