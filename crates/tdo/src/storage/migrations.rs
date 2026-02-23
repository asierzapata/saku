use std::path::PathBuf;

use serde_json::Value;

use crate::storage::StorageError;

type MigrationFn = fn(Value) -> Result<Value, StorageError>;

fn get_migrations() -> Vec<MigrationFn> {
    vec![
        migrate_v1_to_v2,
        migrate_v2_to_v3,
        migrate_v3_to_v4,
        migrate_v4_to_v5,
        migrate_v5_to_v6,
        migrate_v6_to_v7,
        migrate_v7_to_v8,
    ]
}

fn migrate_v1_to_v2(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(2));

        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            // Build indices sorted by created_at for stable numbering
            let mut indices: Vec<usize> = (0..tasks.len()).collect();
            indices.sort_by(|&a, &b| {
                let ts_a = tasks[a]
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ts_b = tasks[b]
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                ts_a.cmp(ts_b)
            });

            // Assign task_number in created_at order, starting at 1
            for (number, &idx) in indices.iter().enumerate() {
                if let Some(task_obj) = tasks[idx].as_object_mut() {
                    task_obj.insert("task_number".to_string(), Value::from((number + 1) as u64));
                }
            }

            // Find the maximum task_number that was assigned
            let max_task_number = tasks
                .iter()
                .filter_map(|t| t.get("task_number").and_then(|v| v.as_u64()))
                .max()
                .unwrap_or(0);
            let next = max_task_number + 1;
            obj.insert("next_task_number".to_string(), Value::from(next));
        } else {
            obj.insert("next_task_number".to_string(), Value::from(1u64));
        }
    }

    Ok(value)
}

fn migrate_v2_to_v3(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(3));

        // Add deleted_at: null to all projects
        if let Some(projects) = obj.get_mut("projects").and_then(|p| p.as_array_mut()) {
            for project in projects {
                if let Some(project_obj) = project.as_object_mut() {
                    project_obj.insert("deleted_at".to_string(), Value::Null);
                }
            }
        }

        // Add deleted_at: null to all areas
        if let Some(areas) = obj.get_mut("areas").and_then(|a| a.as_array_mut()) {
            for area in areas {
                if let Some(area_obj) = area.as_object_mut() {
                    area_obj.insert("deleted_at".to_string(), Value::Null);
                }
            }
        }
    }

    Ok(value)
}

fn migrate_v3_to_v4(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(4));

        // Add modified_at to all tasks, seeded from created_at
        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            for task in tasks {
                if let Some(task_obj) = task.as_object_mut() {
                    let wall_ms = task_obj
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<jiff::Timestamp>().ok())
                        .map(|ts| ts.as_millisecond())
                        .unwrap_or(0);

                    task_obj.insert(
                        "modified_at".to_string(),
                        serde_json::json!({
                            "wall_ms": wall_ms,
                            "lamport": 0,
                            "device_id": ""
                        }),
                    );
                }
            }
        }

        // Add modified_at to all projects, seeded from created_at
        if let Some(projects) = obj.get_mut("projects").and_then(|p| p.as_array_mut()) {
            for project in projects {
                if let Some(project_obj) = project.as_object_mut() {
                    let wall_ms = project_obj
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<jiff::Timestamp>().ok())
                        .map(|ts| ts.as_millisecond())
                        .unwrap_or(0);

                    project_obj.insert(
                        "modified_at".to_string(),
                        serde_json::json!({
                            "wall_ms": wall_ms,
                            "lamport": 0,
                            "device_id": ""
                        }),
                    );
                }
            }
        }

        // Add modified_at to all areas (areas lack created_at, so use 0)
        if let Some(areas) = obj.get_mut("areas").and_then(|a| a.as_array_mut()) {
            for area in areas {
                if let Some(area_obj) = area.as_object_mut() {
                    area_obj.insert(
                        "modified_at".to_string(),
                        serde_json::json!({
                            "wall_ms": 0,
                            "lamport": 0,
                            "device_id": ""
                        }),
                    );
                }
            }
        }
    }

    Ok(value)
}

fn migrate_v4_to_v5(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(5));

        // Remove the evening field from any Scheduled tasks
        // This is handled transparently by serde deserialization,
        // but we explicitly clean up the JSON here for consistency
        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            for task in tasks {
                if let Some(task_obj) = task.as_object_mut()
                    && let Some(when_obj) = task_obj.get_mut("when").and_then(|w| w.as_object_mut())
                    && when_obj.get("type").and_then(|t| t.as_str()) == Some("Scheduled")
                {
                    // Remove evening field if present
                    when_obj.remove("evening");
                }
            }
        }
    }

    Ok(value)
}

fn migrate_v5_to_v6(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(6));

        // Add depends_on: [] to all tasks
        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            for task in tasks {
                if let Some(task_obj) = task.as_object_mut() {
                    task_obj
                        .entry("depends_on")
                        .or_insert_with(|| Value::Array(vec![]));
                }
            }
        }
    }

    Ok(value)
}

fn migrate_v6_to_v7(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(7));

        // Add parent_task_id: null to all tasks, remove checklist field.
        // recurrence and completed_occurrences are handled by serde(default) on Task.
        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            for task in tasks {
                if let Some(task_obj) = task.as_object_mut() {
                    task_obj
                        .entry("parent_task_id")
                        .or_insert(Value::Null);
                    task_obj.remove("checklist");
                }
            }
        }
    }

    Ok(value)
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

fn migrate_v7_to_v8(mut value: Value) -> Result<Value, StorageError> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_string(), Value::from(8));
        if let Some(tasks) = obj.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            let dedupe_next = fix_duplicate_task_numbers(tasks);
            let current_ntn = obj.get("next_task_number").and_then(|v| v.as_u64()).unwrap_or(1);
            obj.insert(
                "next_task_number".to_string(),
                Value::from(current_ntn.max(dedupe_next)),
            );
        }
    }
    Ok(value)
}

/// Returns 1 if version field is missing (assumes v1, our first versioned schema)
pub fn detect_version(content: &str) -> Result<u32, StorageError> {
    let value: Value = serde_json::from_str(content).map_err(|e| StorageError::ParseFailed {
        path: PathBuf::from("<unknown>"),
        source: e,
    })?;

    match value.get("version") {
        Some(v) => v.as_u64().map(|n| n as u32).ok_or_else(|| {
            // Create a dummy parse error since serde_json::Error doesn't have a simple constructor
            let dummy_err = serde_json::from_str::<Value>("invalid").unwrap_err();
            StorageError::ParseFailed {
                path: PathBuf::from("<unknown>"),
                source: dummy_err,
            }
        }),
        None => Ok(1), // No version field = v1
    }
}

/// Migrations are applied sequentially: v1→v2→v3→...→target
pub fn apply_migrations(
    mut data: Value,
    from_version: u32,
    to_version: u32,
) -> Result<Value, StorageError> {
    if from_version == to_version {
        return Ok(data);
    }

    if from_version > to_version {
        return Err(StorageError::FutureVersion(from_version));
    }

    let migrations = get_migrations();

    // Apply each migration in sequence
    for version in from_version..to_version {
        let migration_idx = (version - 1) as usize; // v1→v2 is at index 0

        if migration_idx >= migrations.len() {
            return Err(StorageError::UnsupportedVersion(version));
        }

        data = migrations[migration_idx](data)?;
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_v7_to_v8_fixes_duplicate_task_numbers() {
        let v7_json = serde_json::json!({
            "version": 7,
            "next_task_number": 108,
            "tasks": [
                {
                    "id": "aaa",
                    "task_number": 1,
                    "title": "Oldest",
                    "created_at": "2025-01-01T00:00:00Z",
                    "modified_at": {"wall_ms": 0, "lamport": 0, "device_id": ""}
                },
                {
                    "id": "bbb",
                    "task_number": 1,
                    "title": "Middle",
                    "created_at": "2025-01-02T00:00:00Z",
                    "modified_at": {"wall_ms": 0, "lamport": 0, "device_id": ""}
                },
                {
                    "id": "ccc",
                    "task_number": 1,
                    "title": "Newest",
                    "created_at": "2025-01-03T00:00:00Z",
                    "modified_at": {"wall_ms": 0, "lamport": 0, "device_id": ""}
                }
            ],
            "projects": [],
            "areas": []
        });

        let result = migrate_v7_to_v8(v7_json).unwrap();

        assert_eq!(result["version"], 8);

        let tasks = result["tasks"].as_array().unwrap();
        let mut numbers: Vec<u64> = tasks
            .iter()
            .filter_map(|t| t["task_number"].as_u64())
            .collect();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), 3, "All task numbers must be unique");

        let oldest = tasks.iter().find(|t| t["id"] == "aaa").unwrap();
        assert_eq!(oldest["task_number"], 1, "Oldest task must keep its number");

        assert!(result["next_task_number"].as_u64().unwrap() >= 108);
    }

    #[test]
    fn test_detect_version_with_version_field() {
        let json = r#"{"version": 2, "tasks": [], "projects": [], "areas": []}"#;
        assert_eq!(detect_version(json).unwrap(), 2);
    }

    #[test]
    fn test_detect_version_without_version_field() {
        let json = r#"{"tasks": [], "projects": [], "areas": []}"#;
        assert_eq!(detect_version(json).unwrap(), 1);
    }

    #[test]
    fn test_apply_migrations_same_version() {
        let data = serde_json::json!({"version": 1});
        let result = apply_migrations(data.clone(), 1, 1).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_apply_migrations_future_version() {
        let data = serde_json::json!({"version": 5});
        let result = apply_migrations(data, 5, 1);
        assert!(matches!(result, Err(StorageError::FutureVersion(5))));
    }

    #[test]
    fn test_migrate_v3_to_v4_adds_modified_at() {
        let v3_json = serde_json::json!({
            "version": 3,
            "next_task_number": 2,
            "tasks": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "task_number": 1,
                    "title": "Test task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Inbox"},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-01T12:00:00Z"
                }
            ],
            "projects": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "name": "Test project",
                    "area_id": null, "notes": null, "deadline": null,
                    "completed_at": null, "deleted_at": null,
                    "created_at": "2025-05-01T00:00:00Z"
                }
            ],
            "areas": [
                {
                    "id": "00000000-0000-0000-0000-000000000003",
                    "name": "Test area",
                    "deleted_at": null
                }
            ]
        });

        let result = migrate_v3_to_v4(v3_json).unwrap();

        // Check version bumped
        assert_eq!(result["version"], 4);

        // Check task modified_at
        let task_modified = &result["tasks"][0]["modified_at"];
        // 2025-06-01T12:00:00Z in milliseconds
        let expected_ms: i64 = "2025-06-01T12:00:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_millisecond();
        assert_eq!(task_modified["wall_ms"], expected_ms);
        assert_eq!(task_modified["lamport"], 0);
        assert_eq!(task_modified["device_id"], "");

        // Check project modified_at
        let project_modified = &result["projects"][0]["modified_at"];
        let expected_project_ms: i64 = "2025-05-01T00:00:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_millisecond();
        assert_eq!(project_modified["wall_ms"], expected_project_ms);
        assert_eq!(project_modified["lamport"], 0);
        assert_eq!(project_modified["device_id"], "");

        // Check area modified_at (no created_at, so wall_ms should be 0)
        let area_modified = &result["areas"][0]["modified_at"];
        assert_eq!(area_modified["wall_ms"], 0);
        assert_eq!(area_modified["lamport"], 0);
        assert_eq!(area_modified["device_id"], "");
    }

    #[test]
    fn test_migrate_v1_to_v2_assigns_task_numbers() {
        let v1_json = serde_json::json!({
            "tasks": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Later task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Inbox"},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-02T00:00:00Z"
                },
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "title": "Earlier task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Inbox"},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-01T00:00:00Z"
                }
            ],
            "projects": [],
            "areas": []
        });

        let result = migrate_v1_to_v2(v1_json).unwrap();

        assert_eq!(result["version"], 2);
        assert_eq!(result["next_task_number"], 3);

        // Earlier task (index 1) gets task_number 1, later task (index 0) gets 2
        assert_eq!(result["tasks"][1]["task_number"], 1);
        assert_eq!(result["tasks"][0]["task_number"], 2);
    }

    #[test]
    fn test_migrate_v2_to_v3_adds_deleted_at() {
        let v2_json = serde_json::json!({
            "version": 2,
            "next_task_number": 1,
            "tasks": [],
            "projects": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "name": "My Project",
                    "area_id": null, "notes": null, "deadline": null,
                    "completed_at": null,
                    "created_at": "2025-06-01T00:00:00Z"
                }
            ],
            "areas": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "name": "My Area"
                }
            ]
        });

        let result = migrate_v2_to_v3(v2_json).unwrap();

        assert_eq!(result["version"], 3);
        assert!(result["projects"][0]["deleted_at"].is_null());
        assert!(result["areas"][0]["deleted_at"].is_null());
    }

    #[test]
    fn test_migrate_v4_to_v5_removes_evening() {
        let v4_json = serde_json::json!({
            "version": 4,
            "next_task_number": 2,
            "tasks": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "task_number": 1,
                    "title": "Evening task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Scheduled", "date": "2025-06-15", "evening": true},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-01T00:00:00Z",
                    "modified_at": {"wall_ms": 0, "lamport": 0, "device_id": ""}
                }
            ],
            "projects": [],
            "areas": []
        });

        let result = migrate_v4_to_v5(v4_json).unwrap();

        assert_eq!(result["version"], 5);
        // evening field should be removed
        assert!(result["tasks"][0]["when"].get("evening").is_none());
        // date should still be there
        assert_eq!(result["tasks"][0]["when"]["date"], "2025-06-15");
        assert_eq!(result["tasks"][0]["when"]["type"], "Scheduled");
    }

    #[test]
    fn test_full_migration_v1_to_v5() {
        let v1_json = serde_json::json!({
            "tasks": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "My task",
                    "notes": null, "project_id": null, "area_id": null,
                    "tags": [], "when": {"type": "Scheduled", "date": "2025-06-15", "evening": true},
                    "deadline": null, "defer_until": null,
                    "checklist": [], "completed_at": null, "deleted_at": null,
                    "created_at": "2025-06-01T00:00:00Z"
                }
            ],
            "projects": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "name": "Project",
                    "area_id": null, "notes": null, "deadline": null,
                    "completed_at": null,
                    "created_at": "2025-05-01T00:00:00Z"
                }
            ],
            "areas": [
                {
                    "id": "00000000-0000-0000-0000-000000000003",
                    "name": "Area"
                }
            ]
        });

        let result = apply_migrations(v1_json, 1, 5).unwrap();

        assert_eq!(result["version"], 5);
        // v1→v2: task_number assigned
        assert_eq!(result["tasks"][0]["task_number"], 1);
        assert_eq!(result["next_task_number"], 2);
        // v2→v3: deleted_at on projects/areas
        assert!(result["projects"][0]["deleted_at"].is_null());
        assert!(result["areas"][0]["deleted_at"].is_null());
        // v3→v4: modified_at added
        assert!(result["tasks"][0].get("modified_at").is_some());
        assert!(result["projects"][0].get("modified_at").is_some());
        assert!(result["areas"][0].get("modified_at").is_some());
        // v4→v5: evening removed
        assert!(result["tasks"][0]["when"].get("evening").is_none());
    }
}
