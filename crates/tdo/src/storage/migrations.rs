use std::path::PathBuf;

use serde_json::Value;

use crate::storage::StorageError;

type MigrationFn = fn(Value) -> Result<Value, StorageError>;

fn get_migrations() -> Vec<MigrationFn> {
    vec![migrate_v1_to_v2, migrate_v2_to_v3]
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

            let next = (tasks.len() as u64) + 1;
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
}
