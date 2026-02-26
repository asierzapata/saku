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
            let ca = tasks[a]
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cb = tasks[b]
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
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

/// Extract entity name from a JSON entity.
fn extract_entity_name(entity: &Value) -> Option<String> {
    entity
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
}

/// Deduplicate entities by name (case-insensitive), keeping the one with greatest `modified_at`.
/// Unlike soft-delete approach, this fully merges duplicates by removing losers entirely.
/// Entities without a name field are kept as-is.
fn deduplicate_by_name(entities: Vec<Value>) -> Vec<Value> {
    use std::collections::HashMap;

    let mut by_name: HashMap<String, &Value> = HashMap::new();
    let mut no_name: Vec<&Value> = Vec::new();

    // Group entities by name (case-insensitive), excluding already-deleted ones
    for entity in &entities {
        // Skip already-deleted entities
        if entity.get("deleted_at").is_some_and(|v| !v.is_null()) {
            continue;
        }

        match extract_entity_name(entity) {
            Some(name) => {
                match by_name.get(&name) {
                    Some(existing) => {
                        // Keep the one with more recent modified_at (LWW)
                        if compare_modified_at(entity, existing) == std::cmp::Ordering::Greater {
                            by_name.insert(name, entity);
                        }
                    }
                    None => {
                        by_name.insert(name, entity);
                    }
                }
            }
            None => {
                // Keep entities without names as-is
                no_name.push(entity);
            }
        }
    }

    // Combine deduplicated named entities with unnamed entities
    by_name.into_values().chain(no_name).cloned().collect()
}

/// Build a mapping of old IDs to new IDs when entities are about to be deduplicated by name.
/// Returns a map of loser_id -> winner_id for all entities that will be merged.
fn build_id_mapping(entities: &[Value]) -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;

    let mut by_name: HashMap<String, Vec<&Value>> = HashMap::new();
    let mut id_mapping: HashMap<String, String> = HashMap::new();

    // Group non-deleted entities by name
    for entity in entities {
        // Skip already-deleted entities
        if entity.get("deleted_at").is_some_and(|v| !v.is_null()) {
            continue;
        }

        if let Some(name) = extract_entity_name(entity) {
            by_name.entry(name).or_default().push(entity);
        }
    }

    // For each name with duplicates, map loser IDs to winner ID
    for entities_with_same_name in by_name.values() {
        if entities_with_same_name.len() > 1 {
            // Find the winner (most recent modified_at)
            let winner = entities_with_same_name
                .iter()
                .max_by(|a, b| compare_modified_at(a, b))
                .unwrap();

            let winner_id = winner
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Map all loser IDs to the winner ID
            for entity in entities_with_same_name {
                if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
                    let id_str = id.to_string();
                    if id_str != winner_id {
                        id_mapping.insert(id_str, winner_id.clone());
                    }
                }
            }
        }
    }

    id_mapping
}

/// Reassign references when projects or areas have been merged.
/// Updates task.project_id, task.area_id, and project.area_id fields to point to winning entities.
fn reassign_entity_references(
    mut store: Value,
    project_mapping: &std::collections::HashMap<String, String>,
    area_mapping: &std::collections::HashMap<String, String>,
) -> Value {
    // Reassign tasks that point to merged projects or areas
    if let Some(tasks) = store.get_mut("tasks").and_then(|v| v.as_array_mut()) {
        for task in tasks.iter_mut() {
            // Reassign project_id if it points to a merged project
            if let Some(project_id) = task.get("project_id").and_then(|v| v.as_str())
                && let Some(new_id) = project_mapping.get(project_id) {
                    task["project_id"] = Value::String(new_id.clone());
                }

            // Reassign area_id if it points to a merged area
            if let Some(area_id) = task.get("area_id").and_then(|v| v.as_str())
                && let Some(new_id) = area_mapping.get(area_id) {
                    task["area_id"] = Value::String(new_id.clone());
                }
        }
    }

    // Reassign projects that point to merged areas
    if !area_mapping.is_empty()
        && let Some(projects) = store.get_mut("projects").and_then(|v| v.as_array_mut()) {
            for project in projects.iter_mut() {
                // Reassign area_id if it points to a merged area
                if let Some(area_id) = project.get("area_id").and_then(|v| v.as_str())
                    && let Some(new_id) = area_mapping.get(area_id) {
                        project["area_id"] = Value::String(new_id.clone());
                    }
            }
        }

    store
}

/// Merge two complete store JSON values using LWW for tasks, projects, areas.
/// Takes the maximum of `next_task_number` and deduplicates any task numbers
/// introduced by parallel offline creation on multiple devices.
/// Projects and areas are also deduplicated by name, with tasks reassigned to winning projects/areas.
pub fn lww_merge_store_json(local: &Value, remote: &Value) -> Value {
    use std::collections::HashMap;

    let mut result = local.clone();
    let mut project_id_mapping: HashMap<String, String> = HashMap::new();
    let mut area_id_mapping: HashMap<String, String> = HashMap::new();

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

        let (merged, id_mapping) = match *key {
            "projects" | "areas" => {
                // For projects and areas: merge by ID, then deduplicate by name
                let id_merged = lww_merge_entity_array(&local_arr, &remote_arr);
                let mapping = build_id_mapping(&id_merged);
                let name_deduped = deduplicate_by_name(id_merged);
                (name_deduped, mapping)
            }
            _ => {
                // For tasks: only merge by ID
                (
                    lww_merge_entity_array(&local_arr, &remote_arr),
                    HashMap::new(),
                )
            }
        };

        result[key] = Value::Array(merged);

        // Store ID mappings for later task reassignment
        if *key == "projects" {
            project_id_mapping = id_mapping;
        } else if *key == "areas" {
            area_id_mapping = id_mapping;
        }
    }

    // Reassign tasks that point to merged projects or areas
    if !project_id_mapping.is_empty() || !area_id_mapping.is_empty() {
        result = reassign_entity_references(result, &project_id_mapping, &area_id_mapping);
    }

    // Fix duplicate task_numbers introduced by parallel offline creation
    let mut tasks_arr = result["tasks"].as_array().cloned().unwrap_or_default();
    let dedupe_next = fix_duplicate_task_numbers(&mut tasks_arr);
    result["tasks"] = Value::Array(tasks_arr);

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

        // After merge, only the winner should remain (not soft-deleted)
        assert_eq!(
            projects.len(),
            1,
            "Only one project should remain after merge"
        );
        assert_eq!(
            projects[0]["id"], "proj-bbb",
            "Newer project (by modified_at) wins"
        );
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

        // After merge, only the winner should remain (not soft-deleted)
        assert_eq!(areas.len(), 1, "Only one area should remain after merge");
        assert_eq!(
            areas[0]["id"], "area-bbb",
            "Newer area (by modified_at) wins"
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

        // After merge: only 2 projects should remain (proj-bbb wins over proj-aaa, proj-ccc unaffected)
        assert_eq!(projects.len(), 2, "Blog + newer website should remain");

        let project_ids: Vec<&str> = projects.iter().filter_map(|p| p["id"].as_str()).collect();
        assert!(
            project_ids.contains(&"proj-bbb"),
            "Newer website (proj-bbb) wins"
        );
        assert!(project_ids.contains(&"proj-ccc"), "Blog is unaffected");
        assert!(
            !project_ids.contains(&"proj-aaa"),
            "Older website (proj-aaa) is removed"
        );
    }

    #[test]
    fn test_task_reassignment_on_project_merge() {
        let local = json!({
            "version": 4,
            "next_task_number": 3,
            "tasks": [
                {
                    "id": "task-1",
                    "title": "Task in Project A",
                    "project_id": "proj-a",
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                },
                {
                    "id": "task-2",
                    "title": "Task in Project B",
                    "project_id": "proj-b",
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-b"}
                }
            ],
            "projects": [
                {
                    "id": "proj-a",
                    "name": "Website",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "areas": []
        });

        let remote = json!({
            "version": 4,
            "next_task_number": 2,
            "tasks": [],
            "projects": [
                {
                    "id": "proj-b",
                    "name": "Website",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                }
            ],
            "areas": []
        });

        let merged = lww_merge_store_json(&local, &remote);

        // Should have 1 project (proj-b wins due to newer timestamp)
        let projects = merged["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], "proj-b");

        // Both tasks should now point to proj-b
        let tasks = merged["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["project_id"], "proj-b");
        assert_eq!(tasks[1]["project_id"], "proj-b");
    }

    #[test]
    fn test_task_reassignment_on_area_merge() {
        let local = json!({
            "version": 4,
            "next_task_number": 2,
            "tasks": [
                {
                    "id": "task-1",
                    "title": "Task in Area A",
                    "area_id": "area-a",
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "projects": [],
            "areas": [
                {
                    "id": "area-a",
                    "name": "Work",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ]
        });

        let remote = json!({
            "version": 4,
            "next_task_number": 1,
            "tasks": [],
            "projects": [],
            "areas": [
                {
                    "id": "area-b",
                    "name": "Work",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                }
            ]
        });

        let merged = lww_merge_store_json(&local, &remote);

        // Should have 1 area (area-b wins)
        let areas = merged["areas"].as_array().unwrap();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0]["id"], "area-b");

        // Task should now point to area-b
        let tasks = merged["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["area_id"], "area-b");
    }

    #[test]
    fn test_project_reassignment_on_area_merge() {
        // Test that when two areas with same name merge, projects pointing to losing area get reassigned
        let local = json!({
            "version": 4,
            "next_task_number": 1,
            "tasks": [],
            "projects": [
                {
                    "id": "proj-1",
                    "name": "Website",
                    "area_id": "area-a",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "areas": [
                {
                    "id": "area-a",
                    "name": "Work",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ]
        });

        let remote = json!({
            "version": 4,
            "next_task_number": 1,
            "tasks": [],
            "projects": [
                {
                    "id": "proj-2",
                    "name": "App",
                    "area_id": "area-b",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                }
            ],
            "areas": [
                {
                    "id": "area-b",
                    "name": "Work",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                }
            ]
        });

        let merged = lww_merge_store_json(&local, &remote);

        // Should have 1 area (area-b wins)
        let areas = merged["areas"].as_array().unwrap();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0]["id"], "area-b");

        // Should have 2 projects, both pointing to area-b
        let projects = merged["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0]["area_id"], "area-b");
        assert_eq!(projects[1]["area_id"], "area-b");
    }

    #[test]
    fn test_merge_store_with_duplicate_projects_and_tasks() {
        // Simulate Device A creating "Website" project + 2 tasks
        let device_a = json!({
            "version": 4,
            "next_task_number": 3,
            "tasks": [
                {
                    "id": "task-a1",
                    "title": "Setup frontend",
                    "project_id": "proj-dev-a",
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                },
                {
                    "id": "task-a2",
                    "title": "Setup backend",
                    "project_id": "proj-dev-a",
                    "modified_at": {"wall_ms": 101, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "projects": [
                {
                    "id": "proj-dev-a",
                    "name": "Website",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 100, "lamport": 1, "device_id": "dev-a"}
                }
            ],
            "areas": []
        });

        // Simulate Device B creating "Website" project + 3 tasks
        let device_b = json!({
            "version": 4,
            "next_task_number": 4,
            "tasks": [
                {
                    "id": "task-b1",
                    "title": "Design mockups",
                    "project_id": "proj-dev-b",
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                },
                {
                    "id": "task-b2",
                    "title": "User testing",
                    "project_id": "proj-dev-b",
                    "modified_at": {"wall_ms": 201, "lamport": 1, "device_id": "dev-b"}
                },
                {
                    "id": "task-b3",
                    "title": "Deploy",
                    "project_id": "proj-dev-b",
                    "modified_at": {"wall_ms": 202, "lamport": 1, "device_id": "dev-b"}
                }
            ],
            "projects": [
                {
                    "id": "proj-dev-b",
                    "name": "Website",
                    "deleted_at": null,
                    "modified_at": {"wall_ms": 200, "lamport": 1, "device_id": "dev-b"}
                }
            ],
            "areas": []
        });

        let merged = lww_merge_store_json(&device_a, &device_b);

        // Should have 1 project (proj-dev-b wins - more recent)
        let projects = merged["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], "proj-dev-b");
        assert_eq!(projects[0]["name"], "Website");

        // Should have 5 tasks total, all pointing to proj-dev-b
        let tasks = merged["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 5);

        for task in tasks {
            assert_eq!(task["project_id"], "proj-dev-b");
        }

        // next_task_number should be max of both
        assert_eq!(merged["next_task_number"], 4);
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
