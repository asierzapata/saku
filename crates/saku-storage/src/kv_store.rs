use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entity::EntitySchema;

/// On-disk KV store format. Replaces the old array-based StoredStore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,
}

impl KvStore {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            entries: HashMap::new(),
        }
    }
}

/// Compare two JSON values by their `modified_at` hybrid timestamp.
///
/// Extracts `modified_at.{wall_ms, lamport, device_id}` from each value
/// and compares them in order: wall_ms > lamport > device_id.
/// Returns `Ordering::Equal` if either value lacks `modified_at`.
pub fn compare_modified_at(a: &Value, b: &Value) -> Ordering {
    fn extract(v: &Value) -> Option<(i64, u64, &str)> {
        let m = v.get("modified_at")?;
        let wall_ms = m.get("wall_ms")?.as_i64()?;
        let lamport = m.get("lamport")?.as_u64()?;
        let device_id = m.get("device_id")?.as_str()?;
        Some((wall_ms, lamport, device_id))
    }

    match (extract(a), extract(b)) {
        (Some(a_ts), Some(b_ts)) => a_ts.cmp(&b_ts),
        _ => Ordering::Equal,
    }
}

/// Generic last-writer-wins merge on KV entries.
///
/// For each key present in both stores, the entry with the higher
/// `modified_at` timestamp wins. Keys present in only one store
/// are included as-is. Tombstoned entries participate normally —
/// they win if their `modified_at` is higher.
pub fn lww_merge_kv(local: &KvStore, remote: &KvStore) -> KvStore {
    let mut merged = local.entries.clone();

    for (key, remote_value) in &remote.entries {
        match merged.get(key) {
            Some(local_value) => {
                if compare_modified_at(remote_value, local_value) == Ordering::Greater {
                    merged.insert(key.clone(), remote_value.clone());
                }
            }
            None => {
                merged.insert(key.clone(), remote_value.clone());
            }
        }
    }

    KvStore {
        version: local.version.max(remote.version),
        entries: merged,
    }
}

/// Reconcile renames by following `renamed_to` chains.
///
/// When an entry is a tombstone with `renamed_to`, the target entry
/// should exist. If two devices rename the same entity to different
/// names concurrently, the one with the higher `modified_at` on its
/// tombstone wins. The losing rename's target is itself tombstoned
/// and its references redirected to the winner.
///
/// Chain depth is capped at 10 to prevent infinite loops.
pub fn reconcile_renames(store: &mut KvStore) {
    const MAX_CHAIN_DEPTH: usize = 10;

    // Collect all tombstones with renamed_to
    let rename_tombstones: Vec<(String, String, Value)> = store
        .entries
        .iter()
        .filter(|(_, v)| v.get("deleted_at").is_some() && v.get("renamed_to").is_some())
        .map(|(k, v)| {
            let target = v["renamed_to"].as_str().unwrap_or("").to_string();
            (k.clone(), target, v.clone())
        })
        .collect();

    // Group tombstones by their original key (previous_key on the target)
    // to detect concurrent renames of the same entity
    let mut renames_by_source: HashMap<String, Vec<(String, String, Value)>> = HashMap::new();
    for (old_key, new_key, tombstone) in &rename_tombstones {
        // The "source" is the old_key itself — multiple tombstones for
        // the same old_key means concurrent renames
        renames_by_source
            .entry(old_key.clone())
            .or_default()
            .push((old_key.clone(), new_key.clone(), tombstone.clone()));
    }

    // For concurrent renames (same source, different targets), pick winner
    for renames in renames_by_source.values() {
        if renames.len() <= 1 {
            continue;
        }

        // Pick the rename with the highest modified_at on the tombstone
        let winner = renames
            .iter()
            .max_by(|a, b| compare_modified_at(&a.2, &b.2))
            .unwrap();

        // Loser renames: tombstone their targets and redirect to winner's target
        for rename in renames {
            if rename.1 == winner.1 {
                continue;
            }
            let loser_target_key = &rename.1;
            if let Some(loser_target) = store.entries.get_mut(loser_target_key) {
                // Tombstone the loser's target and point it to the winner
                loser_target["deleted_at"] = winner.2["deleted_at"].clone();
                loser_target["renamed_to"] = Value::String(winner.1.clone());
            }
            // Update the loser tombstone to also point to winner's target
            if let Some(loser_tombstone) = store.entries.get_mut(&rename.0) {
                loser_tombstone["renamed_to"] = Value::String(winner.1.clone());
            }
        }
    }

    // Resolve rename chains: follow renamed_to until we reach a live entry
    let keys: Vec<String> = store
        .entries
        .keys()
        .filter(|k| {
            let v = &store.entries[*k];
            v.get("deleted_at").is_some() && v.get("renamed_to").is_some()
        })
        .cloned()
        .collect();

    for key in keys {
        let mut current = key.clone();
        let mut depth = 0;
        let mut final_target = None;

        while depth < MAX_CHAIN_DEPTH {
            if let Some(entry) = store.entries.get(&current) {
                if let Some(target) = entry.get("renamed_to").and_then(|v| v.as_str()) {
                    let target = target.to_string();
                    if target == current {
                        break; // Self-reference, stop
                    }
                    current = target;
                    depth += 1;
                } else {
                    final_target = Some(current.clone());
                    break;
                }
            } else {
                break;
            }
        }

        // If the chain resolved and the final target differs from the
        // immediate renamed_to, update the tombstone to point directly
        // to the final target (shortcut the chain).
        if let Some(final_key) = final_target
            && let Some(entry) = store.entries.get_mut(&key)
            && let Some(renamed_to) = entry.get("renamed_to").and_then(|v| v.as_str())
            && renamed_to != final_key
        {
            entry["renamed_to"] = Value::String(final_key);
        }
    }
}

/// Repair dangling references by following `renamed_to` chains in tombstones.
///
/// For each entity type in the schemas, scan all entries for reference
/// fields that point to tombstoned keys. Follow the `renamed_to` chain
/// to find the live target and update the reference.
pub fn repair_references(store: &mut KvStore, schemas: &[EntitySchema]) {
    const MAX_CHAIN_DEPTH: usize = 10;

    // Build a map of old_key → final_target for all rename tombstones
    let mut rename_map: HashMap<String, String> = HashMap::new();
    for (key, value) in &store.entries {
        if value.get("deleted_at").is_some()
            && let Some(target) = value.get("renamed_to").and_then(|v| v.as_str())
        {
            rename_map.insert(key.clone(), target.to_string());
        }
    }

    // Resolve chains in the rename map
    let keys: Vec<String> = rename_map.keys().cloned().collect();
    for key in keys {
        let mut current = rename_map[&key].clone();
        let mut depth = 0;
        while depth < MAX_CHAIN_DEPTH {
            if let Some(next) = rename_map.get(&current) {
                if *next == current {
                    break;
                }
                current = next.clone();
                depth += 1;
            } else {
                break;
            }
        }
        rename_map.insert(key, current);
    }

    if rename_map.is_empty() {
        return;
    }

    // For each schema, check all entries of that type for dangling references
    for schema in schemas {
        let prefix = format!("{}/", schema.entity_type);
        let entry_keys: Vec<String> = store
            .entries
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        for entry_key in entry_keys {
            let mut changed = false;
            let mut entry = store.entries[&entry_key].clone();

            for (field_name, _target_type) in &schema.references {
                // Handle single-value references
                if let Some(ref_value) = entry.get(*field_name).and_then(|v| v.as_str()) {
                    if let Some(new_target) = rename_map.get(ref_value) {
                        entry[*field_name] = Value::String(new_target.clone());
                        changed = true;
                    }
                }
                // Handle array references (e.g., depends_on)
                else if let Some(arr) = entry.get(*field_name).and_then(|v| v.as_array()) {
                    let mut new_arr = arr.clone();
                    let mut arr_changed = false;
                    for item in &mut new_arr {
                        if let Some(ref_str) = item.as_str()
                            && let Some(new_target) = rename_map.get(ref_str)
                        {
                            *item = Value::String(new_target.clone());
                            arr_changed = true;
                        }
                    }
                    if arr_changed {
                        entry[*field_name] = Value::Array(new_arr);
                        changed = true;
                    }
                }
            }

            if changed {
                store.entries.insert(entry_key, entry);
            }
        }
    }
}

/// Remove tombstones older than `max_age_days`.
///
/// A tombstone is an entry with `deleted_at` set. If `modified_at.wall_ms`
/// is older than `now_ms - max_age_days * 86400000`, it is removed.
pub fn gc_tombstones(store: &mut KvStore, max_age_days: u32, now_ms: i64) {
    let cutoff_ms = now_ms - (max_age_days as i64) * 86_400_000;

    store.entries.retain(|_key, value| {
        // Keep non-tombstones
        if value.get("deleted_at").is_none() {
            return true;
        }
        // Keep tombstones that are recent enough
        if let Some(modified_at) = value.get("modified_at")
            && let Some(wall_ms) = modified_at.get("wall_ms").and_then(|v| v.as_i64())
        {
            return wall_ms >= cutoff_ms;
        }
        // No modified_at → keep (can't determine age)
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_entry(wall_ms: i64, lamport: u64, device_id: &str) -> Value {
        json!({
            "name": "test",
            "modified_at": {
                "wall_ms": wall_ms,
                "lamport": lamport,
                "device_id": device_id
            }
        })
    }

    fn make_tombstone(wall_ms: i64, lamport: u64, device_id: &str) -> Value {
        json!({
            "name": "test",
            "deleted_at": "2026-01-01T00:00:00Z",
            "modified_at": {
                "wall_ms": wall_ms,
                "lamport": lamport,
                "device_id": device_id
            }
        })
    }

    fn make_rename_tombstone(
        wall_ms: i64,
        lamport: u64,
        device_id: &str,
        renamed_to: &str,
    ) -> Value {
        json!({
            "name": "test",
            "deleted_at": "2026-01-01T00:00:00Z",
            "renamed_to": renamed_to,
            "modified_at": {
                "wall_ms": wall_ms,
                "lamport": lamport,
                "device_id": device_id
            }
        })
    }

    // ── compare_modified_at ──

    #[test]
    fn compare_modified_at_higher_wall_ms_wins() {
        let a = make_entry(2000, 1, "dev-a");
        let b = make_entry(1000, 1, "dev-a");
        assert_eq!(compare_modified_at(&a, &b), Ordering::Greater);
    }

    #[test]
    fn compare_modified_at_equal_timestamps() {
        let a = make_entry(1000, 1, "dev-a");
        let b = make_entry(1000, 1, "dev-a");
        assert_eq!(compare_modified_at(&a, &b), Ordering::Equal);
    }

    #[test]
    fn compare_modified_at_missing_field_returns_equal() {
        let a = json!({"name": "test"});
        let b = make_entry(1000, 1, "dev-a");
        assert_eq!(compare_modified_at(&a, &b), Ordering::Equal);
    }

    // ── lww_merge_kv ──

    #[test]
    fn lww_merge_kv_same_key_higher_wins() {
        let local = KvStore {
            version: 9,
            entries: HashMap::from([("project/website".into(), make_entry(1000, 1, "dev-a"))]),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::from([("project/website".into(), make_entry(2000, 1, "dev-b"))]),
        };

        let merged = lww_merge_kv(&local, &remote);
        let entry = &merged.entries["project/website"];
        assert_eq!(
            entry["modified_at"]["wall_ms"].as_i64().unwrap(),
            2000,
            "remote with higher wall_ms should win"
        );
    }

    #[test]
    fn lww_merge_kv_same_key_local_wins_when_newer() {
        let local = KvStore {
            version: 9,
            entries: HashMap::from([("project/website".into(), make_entry(3000, 1, "dev-a"))]),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::from([("project/website".into(), make_entry(2000, 1, "dev-b"))]),
        };

        let merged = lww_merge_kv(&local, &remote);
        let entry = &merged.entries["project/website"];
        assert_eq!(entry["modified_at"]["wall_ms"].as_i64().unwrap(), 3000);
    }

    #[test]
    fn lww_merge_kv_new_keys_from_both_sides() {
        let local = KvStore {
            version: 9,
            entries: HashMap::from([("project/alpha".into(), make_entry(1000, 1, "dev-a"))]),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::from([("project/beta".into(), make_entry(1000, 1, "dev-b"))]),
        };

        let merged = lww_merge_kv(&local, &remote);
        assert!(merged.entries.contains_key("project/alpha"));
        assert!(merged.entries.contains_key("project/beta"));
        assert_eq!(merged.entries.len(), 2);
    }

    #[test]
    fn lww_merge_kv_tombstone_wins_over_live_when_newer() {
        let local = KvStore {
            version: 9,
            entries: HashMap::from([("project/old".into(), make_entry(1000, 1, "dev-a"))]),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::from([("project/old".into(), make_tombstone(2000, 1, "dev-a"))]),
        };

        let merged = lww_merge_kv(&local, &remote);
        let entry = &merged.entries["project/old"];
        assert!(entry.get("deleted_at").is_some(), "tombstone should win");
    }

    #[test]
    fn lww_merge_kv_live_wins_over_tombstone_when_newer() {
        let local = KvStore {
            version: 9,
            entries: HashMap::from([("project/old".into(), make_entry(3000, 1, "dev-a"))]),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::from([("project/old".into(), make_tombstone(2000, 1, "dev-a"))]),
        };

        let merged = lww_merge_kv(&local, &remote);
        let entry = &merged.entries["project/old"];
        assert!(
            entry.get("deleted_at").is_none(),
            "live entry should win when newer"
        );
    }

    #[test]
    fn lww_merge_kv_picks_higher_version() {
        let local = KvStore {
            version: 8,
            entries: HashMap::new(),
        };
        let remote = KvStore {
            version: 9,
            entries: HashMap::new(),
        };
        let merged = lww_merge_kv(&local, &remote);
        assert_eq!(merged.version, 9);
    }

    // ── reconcile_renames ──

    #[test]
    fn reconcile_renames_single_rename_is_noop() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                (
                    "project/website".into(),
                    make_rename_tombstone(2000, 1, "dev-a", "project/blog"),
                ),
                ("project/blog".into(), make_entry(2000, 2, "dev-a")),
            ]),
        };

        reconcile_renames(&mut store);

        // Single rename: no changes needed
        assert_eq!(
            store.entries["project/website"]["renamed_to"]
                .as_str()
                .unwrap(),
            "project/blog"
        );
        assert!(store.entries["project/blog"]
            .get("deleted_at")
            .is_none());
    }

    #[test]
    fn reconcile_renames_chain_gets_shortcut() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                (
                    "project/a".into(),
                    make_rename_tombstone(1000, 1, "dev-a", "project/b"),
                ),
                (
                    "project/b".into(),
                    make_rename_tombstone(2000, 1, "dev-a", "project/c"),
                ),
                ("project/c".into(), make_entry(3000, 1, "dev-a")),
            ]),
        };

        reconcile_renames(&mut store);

        // project/a should now point directly to project/c
        assert_eq!(
            store.entries["project/a"]["renamed_to"]
                .as_str()
                .unwrap(),
            "project/c"
        );
    }

    // ── repair_references ──

    #[test]
    fn repair_references_fixes_dangling_single_ref() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                (
                    "project/old".into(),
                    make_rename_tombstone(2000, 1, "dev-a", "project/new"),
                ),
                ("project/new".into(), make_entry(2000, 2, "dev-a")),
                (
                    "task/abc123".into(),
                    json!({
                        "title": "Fix bug",
                        "project_key": "project/old",
                        "modified_at": { "wall_ms": 1500, "lamport": 1, "device_id": "dev-b" }
                    }),
                ),
            ]),
        };

        let schemas = vec![EntitySchema {
            entity_type: "task",
            references: vec![("project_key", "project")],
        }];

        repair_references(&mut store, &schemas);

        assert_eq!(
            store.entries["task/abc123"]["project_key"]
                .as_str()
                .unwrap(),
            "project/new"
        );
    }

    #[test]
    fn repair_references_fixes_array_refs() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                (
                    "task/old1".into(),
                    make_rename_tombstone(2000, 1, "dev-a", "task/new1"),
                ),
                ("task/new1".into(), make_entry(2000, 2, "dev-a")),
                (
                    "task/abc".into(),
                    json!({
                        "title": "Depends on old",
                        "depends_on": ["task/old1", "task/other"],
                        "modified_at": { "wall_ms": 1500, "lamport": 1, "device_id": "dev-b" }
                    }),
                ),
            ]),
        };

        let schemas = vec![EntitySchema {
            entity_type: "task",
            references: vec![("depends_on", "task")],
        }];

        repair_references(&mut store, &schemas);

        let deps = store.entries["task/abc"]["depends_on"]
            .as_array()
            .unwrap();
        assert_eq!(deps[0].as_str().unwrap(), "task/new1");
        assert_eq!(deps[1].as_str().unwrap(), "task/other");
    }

    #[test]
    fn repair_references_follows_chain() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                (
                    "project/a".into(),
                    make_rename_tombstone(1000, 1, "dev-a", "project/b"),
                ),
                (
                    "project/b".into(),
                    make_rename_tombstone(2000, 1, "dev-a", "project/c"),
                ),
                ("project/c".into(), make_entry(3000, 1, "dev-a")),
                (
                    "task/t1".into(),
                    json!({
                        "title": "Old ref",
                        "project_key": "project/a",
                        "modified_at": { "wall_ms": 500, "lamport": 1, "device_id": "dev-b" }
                    }),
                ),
            ]),
        };

        let schemas = vec![EntitySchema {
            entity_type: "task",
            references: vec![("project_key", "project")],
        }];

        repair_references(&mut store, &schemas);

        assert_eq!(
            store.entries["task/t1"]["project_key"].as_str().unwrap(),
            "project/c"
        );
    }

    #[test]
    fn repair_references_no_renames_is_noop() {
        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                ("project/web".into(), make_entry(1000, 1, "dev-a")),
                (
                    "task/t1".into(),
                    json!({
                        "title": "Valid ref",
                        "project_key": "project/web",
                        "modified_at": { "wall_ms": 500, "lamport": 1, "device_id": "dev-a" }
                    }),
                ),
            ]),
        };

        let schemas = vec![EntitySchema {
            entity_type: "task",
            references: vec![("project_key", "project")],
        }];

        repair_references(&mut store, &schemas);

        assert_eq!(
            store.entries["task/t1"]["project_key"].as_str().unwrap(),
            "project/web"
        );
    }

    // ── gc_tombstones ──

    #[test]
    fn gc_tombstones_removes_old_tombstones() {
        let now_ms = 1_000_000_000_000i64; // some timestamp
        let old_ms = now_ms - 31 * 86_400_000; // 31 days ago
        let recent_ms = now_ms - 5 * 86_400_000; // 5 days ago

        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([
                ("project/old".into(), make_tombstone(old_ms, 1, "dev-a")),
                (
                    "project/recent".into(),
                    make_tombstone(recent_ms, 1, "dev-a"),
                ),
                ("project/live".into(), make_entry(1000, 1, "dev-a")),
            ]),
        };

        gc_tombstones(&mut store, 30, now_ms);

        assert!(
            !store.entries.contains_key("project/old"),
            "old tombstone should be removed"
        );
        assert!(
            store.entries.contains_key("project/recent"),
            "recent tombstone should be kept"
        );
        assert!(
            store.entries.contains_key("project/live"),
            "live entry should be kept"
        );
    }

    #[test]
    fn gc_tombstones_keeps_all_when_young() {
        let now_ms = 1_000_000_000_000i64;
        let recent_ms = now_ms - 1 * 86_400_000; // 1 day ago

        let mut store = KvStore {
            version: 9,
            entries: HashMap::from([(
                "project/deleted".into(),
                make_tombstone(recent_ms, 1, "dev-a"),
            )]),
        };

        gc_tombstones(&mut store, 30, now_ms);

        assert!(store.entries.contains_key("project/deleted"));
    }
}
