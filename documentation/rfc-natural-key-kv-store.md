# RFC: Natural-Key-Based Key-Value Store for saku-storage

**Author:** @asierzapata
**Status:** Implemented

## Summary

We propose replacing the current monolithic JSON store (arrays of entities keyed by UUID) with a **natural-key-based key-value store** in `saku-storage`. Instead of relying on UUIDs for identity and running deduplication passes during sync, each entity type declares a **natural key function** — a deterministic derivation of the business-level uniqueness constraint. The storage key becomes `{entity_type}/{natural_key}`, making duplicates impossible by construction. UUIDs are removed entirely from the architecture.

This eliminates the name-based deduplication code in `saku-sync`, makes the sync layer fully generic, and gives us a foundation that can extend beyond JSON files to SQLite, iOS apps, and web clients.

## Context and Problem Statement

### How things work today

Our storage layer is a single JSON file per tool:

```
~/.local/share/tdo/store.json
```

The file contains three entity arrays — tasks, projects, and areas — keyed internally by UUID:

```rust
// crates/tdo/src/models/store.rs:12-18
pub struct StoredStore {
    pub version: u32,
    pub next_task_number: u64,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub areas: Vec<Area>,
}
```

At runtime we convert to `HashMap<Uuid, Entity>` for O(1) lookups (`Store::from_stored()`, `store.rs:56`). UUIDs are the only identity — there is no way for the storage or sync layer to know that two projects named "Website" on different devices are the same project.

### Where this breaks

When two devices create entities offline, each generates a fresh UUID. After sync, we end up with two projects named "Website" — different UUIDs, same business meaning. We fixed this with a two-pass merge in `saku-sync/src/conflict.rs`:

1. **Pass 1** — LWW merge by UUID (`lww_merge_entity_array`, `conflict.rs:12`)
2. **Pass 2** — Name-based deduplication (`deduplicate_by_name`, `conflict.rs:125`), which groups by case-insensitive name, picks the winner by `modified_at`, removes losers, and reassigns foreign references (`reassign_entity_references`, `conflict.rs:215`)

This works, but it has real problems:

**The sync layer knows tdo's business rules.** `deduplicate_by_name` hardcodes that projects and areas have a `name` field, that it should be compared case-insensitively, and that tasks have `project_id` and `area_id` fields that need reassignment. If we build `jrn` (journal) or `dcs` (decisions) with their own entity types and uniqueness rules, we'd need to add more tool-specific code to `saku-sync`.

**Local writes don't enforce uniqueness.** Nothing prevents `Store::add_project()` from creating a second "Website" project on the same device. The check happens only during sync merge — and only for projects and areas, not for any other entity type that might need it.

**The dedup is lossy.** When we merge two "Website" projects, we pick one UUID and discard the other. Any external system that stored the losing UUID now has a dangling reference.

**It doesn't scale to new tools.** Every new CLI that uses `saku-storage` and `saku-sync` would need to either:
- Add its own dedup logic to `conflict.rs` (breaks layering)
- Accept duplicates and deal with them in the application layer (shifts the problem)

### The root cause

The UUID tells us *which record* something is, but not *what it represents*. The business-level uniqueness constraint ("project names are unique, case-insensitive") is encoded only in the tdo application layer and the sync dedup code. The storage and sync layers have no concept of it.

## Proposed Solution: Natural-Key KV Store

### The core idea

Every entity type declares a **natural key function** — a pure function from the entity's fields to a deterministic string key. The storage layer uses `{entity_type}/{natural_key}` as the primary key. Two entities with the same natural key are, by definition, the same entity. UUIDs are removed entirely — they have no place in this architecture.

```rust
// Proposed: in saku-storage
pub trait Entity: Serialize + DeserializeOwned {
    /// The entity type name (e.g., "project", "task", "area").
    fn entity_type() -> &'static str;

    /// Derive the natural key from this entity's fields.
    fn natural_key(&self) -> String;

    /// Full storage key: "{entity_type}/{natural_key}"
    fn storage_key(&self) -> String {
        format!("{}/{}", Self::entity_type(), self.natural_key())
    }
}
```

For tdo's entities, the natural key functions would be:

| Entity | Natural Key | Example Storage Key | User-Facing ID |
| --- | --- | --- | --- |
| Project | `name.to_lowercase()` | `project/website` | "Website" |
| Area | `name.to_lowercase()` | `area/work` | "Work" |
| Task | `hash(device_id, timestamp)` | `task/k7m2a3x9` | #42 |

Projects and areas use their name as both the key and the display identity. Tasks are different — they have no natural business key (multiple tasks can share a title), so they use a short generated hash as the storage key and a sequential number as the user-facing identifier. See the deep dive below for why this split is necessary and how it works.

### What the storage layer looks like

Instead of `Vec<Project>` serialized as a JSON array, we store a flat key-value map:

```rust
// Proposed: replaces StoredStore
pub struct KvStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,   // "project/website" -> { ...project fields... }
}
```

The store is just a version number and a flat map of entries. There's no `metadata` section — values like `next_task_number` are computed at runtime (`max(task_numbers) + 1`), not stored. This means there's nothing tool-specific to sync or merge beyond the entries themselves.

On disk, this serializes as:

```json
{
  "version": 9,
  "entries": {
    "project/website": {
      "name": "Website",
      "area_key": "area/work",
      "modified_at": { "wall_ms": 1000, "lamport": 1, "device_id": "dev-a" }
    },
    "area/work": {
      "name": "Work",
      "modified_at": { "wall_ms": 900, "lamport": 1, "device_id": "dev-a" }
    },
    "task/k7m2a3x9": {
      "task_number": 42,
      "title": "Fix the login bug",
      "project_key": "project/website",
      "modified_at": { "wall_ms": 1100, "lamport": 2, "device_id": "dev-a" }
    }
  }
}
```

No UUIDs anywhere. For projects and areas, the key IS the identity — `"project/website"` is both the storage key and how you reference it. For tasks, the storage key is a short generated hash (`task/k7m2a3x9`) while the user interacts via the sequential `task_number` (42). See the task identity deep dive for why this split is necessary.

Foreign references use the **full storage key** (e.g., `"project/website"`, not just `"website"`), which makes references self-describing and unambiguous regardless of entity type.

### What sync becomes

The sync merge simplifies to a single generic pass:

```rust
// Proposed: replaces lww_merge_store_json + deduplicate_by_name + reassign_entity_references
pub fn lww_merge_kv(local: &KvStore, remote: &KvStore) -> KvStore {
    let mut merged = local.entries.clone();

    for (key, remote_value) in &remote.entries {
        match merged.get(key) {
            Some(local_value) => {
                // Same key = same entity. LWW wins.
                if compare_modified_at(remote_value, local_value) == Ordering::Greater {
                    merged.insert(key.clone(), remote_value.clone());
                }
            }
            None => {
                // New key. Include it.
                merged.insert(key.clone(), remote_value.clone());
            }
        }
    }

    KvStore {
        version: local.version.max(remote.version),
        entries: merged,
    }
}
```

That's it. No name deduplication. No reference reassignment. No entity-type-specific logic. The sync layer becomes fully generic — it works for any tool that implements the `Entity` trait.

After the generic merge, each tool can optionally register a **post-merge hook** for tool-specific cleanup (like fixing task number collisions in tdo). The key distinction: the sync layer does the merge, the tool does the fixup. Clean separation.

### Entity schema registration

Each tool registers its entity schemas with the storage layer:

```rust
// In tdo's initialization
let schemas = vec![
    EntitySchema {
        entity_type: "project",
        natural_key_fn: |value| {
            value["name"].as_str()
                .map(|s| s.to_lowercase())
                .unwrap_or_default()
        },
        references: vec![("area_key", "area")],
    },
    EntitySchema {
        entity_type: "area",
        natural_key_fn: |value| {
            value["name"].as_str()
                .map(|s| s.to_lowercase())
                .unwrap_or_default()
        },
        references: vec![],
    },
    EntitySchema {
        entity_type: "task",
        // Tasks use a generated hash key (not a natural business key).
        // The key is computed at creation time and stored on the entry.
        natural_key_fn: |value| {
            // The key is pre-computed and embedded; just extract it.
            value["_storage_key_suffix"].as_str()
                .unwrap_or_default()
                .to_string()
        },
        references: vec![
            ("project_key", "project"),
            ("area_key", "area"),
            ("parent_task_key", "task"),
        ],
    },
];
```

The storage layer uses these schemas to:
- **Enforce uniqueness on local writes** — reject or merge if a key already exists
- **Validate references** — warn if a `project_key` points to a nonexistent entry
- **Handle renames** — automatically update references when a key changes (see tombstone section)

The sync layer only needs the generic `lww_merge_kv`. It doesn't need schemas, entity types, or any application knowledge.

## Deep Dive: Task Identity in the KV Model

Task identity is the hardest problem in this design. Projects and areas have natural business keys (their name). Tasks don't — two tasks can legitimately have the same title. So what makes a task unique?

The answer: a task is unique because it was created as a distinct action. There's no field on the task that constitutes a natural uniqueness constraint. This means we need some form of **generated identifier** as the storage key. The question is: what form should it take, and how does the user interact with it?

### The fundamental tension

We want three things simultaneously:

1. **Globally unique** — two devices creating tasks offline must never produce the same key
2. **Short and easy to type** — the user types `tdo done <id>` hundreds of times
3. **Sequential** — users develop muscle memory; seeing tasks 1, 2, 3 is natural

These three properties are in tension. Sequential numbers aren't globally unique without coordination. Globally unique identifiers (UUIDs, hashes) aren't sequential. We have to pick our compromise.

### Approaches evaluated

#### Approach A: Sequential numbers as the key (`task/42`)

Both devices allocate sequentially. Collisions resolved somehow.

**Problem:** If the task number IS the storage key, two devices creating `task/42` produces a key collision. LWW picks one, the other task is lost. Catastrophic.

The only way to use sequential numbers as keys is device-partitioned ranges (device A: 1-999, device B: 1000-1999). But this creates jarring gaps — you go from task 3 on your laptop to task 1000 on your phone. Numbers lose their "small and familiar" quality. Users develop muscle memory around low sequential numbers; jumping to 1000 breaks that. **Rejected.**

#### Approach B: Short hash IDs (`task/k7m2`)

Generate a short, globally unique identifier from `{device_id}-{creation_timestamp_ms}`. Encode as 4-6 characters in base-36 (0-9, a-z).

```
tdo add "Fix login bug"    → task/k7m2
tdo add "Update docs"      → task/a3x9
tdo done k7m2
```

| Property | Rating |
| --- | --- |
| Globally unique | Yes — device_id + timestamp guarantees it |
| Short | Yes — 4-6 chars |
| Easy to type | Moderate — alphanumeric, but must look it up each time |
| Sequential | No — `k7m2` tells you nothing about order |
| Easy to remember | Poor — no pattern to anchor on |

**Assessment:** Solves the uniqueness problem cleanly but sacrifices the ergonomics that make task numbers good. You'd always need to run `tdo view` first to look up the ID. For a CLI tool used hundreds of times a day, that friction adds up.

#### Approach C: Sequential numbers as display + short hash as storage key (Recommended)

The pragmatic approach: **separate the storage key from the user-facing identifier.**

- **Storage key:** A short generated hash — globally unique, stable, never changes. This is the KV store identity.
- **Task number:** Sequential, allocated locally, used for all CLI interaction. May differ across devices.

```
Storage:  task/k7m2  →  { task_number: 42, title: "Fix login bug", ... }
CLI:      tdo done 42  (looks up number 42 → key "task/k7m2")
```

The user never sees or types the storage key. They interact exclusively with task numbers, just like today.

| Property | Rating |
| --- | --- |
| Globally unique key | Yes — hash is unique across devices |
| Sequential numbers | Yes — each device allocates 1, 2, 3... |
| Short to type | Yes — `tdo done 42`, same as today |
| Easy to remember | Yes — low sequential numbers, muscle memory works |
| Numbers stable | Mostly — your local numbers never change; see "sync behavior" below |

**This means tasks have a split identity:** the storage key (for the KV store and sync) and the task number (for the user). Projects and areas don't have this split — their name is both the key and the display. But tasks are fundamentally different: they lack a natural business key, so the split is unavoidable.

### How Approach C works in detail

**Task creation:**

```rust
fn create_task(store: &mut KvStore, title: &str) -> Task {
    let device_id = get_device_id();
    let timestamp = now_ms();
    let storage_key = generate_task_key(device_id, timestamp); // e.g., "task/k7m2a3x9"
    let task_number = store.max_task_number() + 1;             // computed from existing entries

    let task = Task { task_number, title, ... };
    store.put(&storage_key, &task);
    task
}
```

**Task lookup by number:**

The store maintains a secondary index `task_number → storage_key`:

```rust
impl Store {
    fn get_task_by_number(&self, number: u64) -> Option<&Task> {
        let key = self.task_number_index.get(&number)?;
        self.tasks.get(key)
    }
}
```

This index is rebuilt from entries on load (not persisted separately). Since task numbers are a field inside the value, rebuilding is a single scan.

**Sync behavior:**

When device A syncs with device B:
1. New tasks from device B arrive with their storage keys (globally unique, no collision)
2. Each new task has a `task_number` that was assigned on device B
3. If that number is already in use locally, the **incoming** task gets the next available number
4. Incoming tasks are numbered in `created_at` order, preserving the creation sequence from the other device
5. The task's storage key stays the same — only the local display number changes
6. `next_task_number` is never synced — each device computes it as `max(local task_numbers) + 1`

```
Before sync (Device A):
  task/k7m2 → { task_number: 1, title: "Fix login" }
  task/a3x9 → { task_number: 2, title: "Update docs" }

Before sync (Device B):
  task/p4b1 → { task_number: 1, title: "Design mockups" }
  task/r2d7 → { task_number: 2, title: "User testing" }

After sync (Device A sees):
  task/k7m2 → { task_number: 1, title: "Fix login" }       ← unchanged
  task/a3x9 → { task_number: 2, title: "Update docs" }     ← unchanged
  task/p4b1 → { task_number: 3, title: "Design mockups" }  ← renumbered locally
  task/r2d7 → { task_number: 4, title: "User testing" }    ← renumbered locally

After sync (Device B sees):
  task/p4b1 → { task_number: 1, title: "Design mockups" }  ← unchanged
  task/r2d7 → { task_number: 2, title: "User testing" }    ← unchanged
  task/k7m2 → { task_number: 3, title: "Fix login" }       ← renumbered locally
  task/a3x9 → { task_number: 4, title: "Update docs" }     ← renumbered locally
```

**Key property:** Your local tasks never change numbers. Only incoming tasks get renumbered. The task you've been calling "42" stays "42" on your device.

**Cross-device number mismatch:** Yes, the same task might be #3 on device A and #1 on device B. This is acceptable because:
- You typically work on one device at a time
- The storage key is the true identity — sync works correctly regardless of local numbers
- If you need to reference a task cross-device, you use the title or project context, not the number
- In practice with a single user, you'll mostly create tasks on one primary device and consume them on another

### Generating the storage key

The storage key must be globally unique, deterministic, and short. We derive it from immutable creation-time data:

```rust
fn generate_task_key(device_id: &str, creation_ms: i64) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update(creation_ms.to_le_bytes());
    let hash = hasher.finalize();

    // Take first 5 bytes, encode as base-36 → 8 characters
    // 5 bytes = 40 bits = ~1 trillion possible values
    let num = u64::from_le_bytes([hash[0], hash[1], hash[2], hash[3], hash[4], 0, 0, 0]);
    format!("task/{}", base36_encode(num))
}
```

Properties of the generated key:
- **5 bytes of entropy**: ~1 trillion possible values. Collision probability is negligible for stores under millions of tasks.
- **Base-36 encoding**: 8 lowercase alphanumeric characters. Short enough that if a user ever needs to see it (debugging), it's manageable.
- **Deterministic**: Same device + same millisecond = same key. Two tasks created in the same millisecond on the same device would collide, but in practice `creation_ms` is unique per task (tasks aren't created in sub-millisecond bursts).

### Impact on the KV store design

With Approach C, the entity table becomes:

| Entity | Natural Key | Example Storage Key | User-Facing ID |
| --- | --- | --- | --- |
| Project | `name.to_lowercase()` | `project/website` | "Website" |
| Area | `name.to_lowercase()` | `area/work` | "Work" |
| Task | `hash(device_id, timestamp)` | `task/k7m2a3x9` | #42 |

Projects and areas have a clean "key = display name" identity. Tasks have a split identity: hash key for storage, sequential number for display. This is an honest reflection of the fact that tasks are fundamentally different from named entities.

## Deep Dive: Tombstones and Renames

Renames are the hardest operation in a natural-key system because changing a field that's part of the key means changing the key itself. We handle this with **tombstones** — entries marked as deleted that remain in the store to prevent resurrection and to serve as forwarding pointers.

### Tombstone basics

A **tombstone** is an entry with `deleted_at` set. It stays in the KV store and participates in merge:

```json
{
  "project/website": {
    "name": "Website",
    "deleted_at": "2026-02-27T10:00:00Z",
    "modified_at": { "wall_ms": 2000, "lamport": 5, "device_id": "dev-a" }
  }
}
```

**Why keep deleted entries?** To prevent resurrection. Without tombstones, if device A deletes a project and device B still has it, the merge would see "device A: absent, device B: present" and include it — undoing the deletion. With tombstones, the merge sees "device A: tombstoned at t=2000, device B: alive at t=1000" and the tombstone wins (higher `modified_at`).

Tombstones are small (just the key + timestamps) and can be garbage-collected after all devices have synced past the tombstone's timestamp. For now, we keep all tombstones indefinitely — they're negligible in size for our data volumes.

### Rename operation

Renaming a project from "Website" to "Blog" changes its natural key from `project/website` to `project/blog`. We handle this as a **tombstone with forwarding pointer + new entry**:

1. **Tombstone the old key** — set `deleted_at` and add a `renamed_to` field pointing to the new key:
   ```json
   "project/website": {
     "deleted_at": "2026-02-27T10:00:00Z",
     "renamed_to": "project/blog",
     "modified_at": { "wall_ms": 2000, "lamport": 5, "device_id": "dev-a" }
   }
   ```

2. **Create the new key** — write the entity data with a `previous_key` field:
   ```json
   "project/blog": {
     "name": "Blog",
     "previous_key": "project/website",
     "area_key": "area/work",
     "modified_at": { "wall_ms": 2000, "lamport": 6, "device_id": "dev-a" }
   }
   ```

3. **Update local references** — scan all entries that have `"project/website"` as a reference value and update them to `"project/blog"`.

The `renamed_to` and `previous_key` fields are the critical pieces. They create a bidirectional link between the old and new keys, enabling automatic reference repair during sync.

### Scenario: Rename on one device, new tasks on another

This is the most common real-world conflict:

```
Device A (offline):                   Device B (offline):
────────────────                      ────────────────
Renames "Website" → "Blog"            Creates task in "Website"
  project/website → tombstone           task/1001 = { project_key: "project/website" }
    renamed_to: "project/blog"
  project/blog → { name: "Blog" }
  updates existing task refs
```

After sync (merge both states):

```
project/website → tombstone, renamed_to: "project/blog"
                  (Device A's tombstone wins: modified_at 2000 > Device B's unchanged 1000)
project/blog    → { name: "Blog" }  (from Device A)
task/1001       → { project_key: "project/website" }  ← DANGLING REFERENCE
```

**Post-merge reference repair:** After the generic LWW merge, a repair pass runs:

1. Collect all tombstones that have a `renamed_to` field
2. For each tombstone, scan all entries for references to the tombstoned key
3. Update those references to point to the `renamed_to` target

Result after repair:
```
task/1001 → { project_key: "project/blog" }  ← FIXED
```

This repair pass is generic — it works on any entity type with references. It uses the schema's `references` list to know which fields to scan, but it doesn't need entity-type-specific logic.

### Scenario: Concurrent renames (same entity, different new names)

This is the hardest case. Device A renames "Website" → "Blog", Device B renames "Website" → "Portfolio":

```
Device A:                              Device B:
────────                               ────────
project/website → tombstone            project/website → tombstone
  renamed_to: "project/blog"             renamed_to: "project/portfolio"
  modified_at: 1000                      modified_at: 2000

project/blog → { name: "Blog"         project/portfolio → { name: "Portfolio"
  previous_key: "project/website"        previous_key: "project/website"
  modified_at: 1000 }                    modified_at: 2000 }
```

After generic LWW merge:

```
project/website   → tombstone, renamed_to: "project/portfolio", modified_at: 2000
                    (Device B wins: higher modified_at)
project/blog      → { name: "Blog", previous_key: "project/website", modified_at: 1000 }
project/portfolio → { name: "Portfolio", previous_key: "project/website", modified_at: 2000 }
```

We now have **two live entries** that both claim to be the successor of `project/website`. The **post-merge rename reconciliation** resolves this:

1. The tombstone at `project/website` says `renamed_to: "project/portfolio"` — this is the **winning rename** (since the tombstone's `modified_at` determined which `renamed_to` value survived LWW).
2. Scan for all entries with `previous_key: "project/website"` → finds `project/blog` and `project/portfolio`.
3. `project/portfolio` matches the tombstone's `renamed_to` → **winner, keep it**.
4. `project/blog` does NOT match → **loser, tombstone it**, with its own `renamed_to: "project/portfolio"`:
   ```json
   "project/blog": {
     "deleted_at": "...",
     "renamed_to": "project/portfolio",
     "modified_at": { "wall_ms": <now>, ... }
   }
   ```
5. Any references to `project/blog` (from Device A's local tasks) are updated to `project/portfolio`.

Final state: one project called "Portfolio", all tasks pointing to it. The user on Device A sees that their rename to "Blog" lost to Device B's rename to "Portfolio" — which is the correct LWW behavior.

### Rename reconciliation algorithm

```rust
fn reconcile_renames(store: &mut KvStore) {
    // 1. Collect all tombstones with renamed_to
    let rename_tombstones: Vec<(String, String)> = store.entries.iter()
        .filter(|(_, v)| v.get("deleted_at").is_some() && v.get("renamed_to").is_some())
        .map(|(k, v)| (k.clone(), v["renamed_to"].as_str().unwrap().to_string()))
        .collect();

    for (old_key, winning_new_key) in &rename_tombstones {
        // 2. Find all entries claiming to be successors of old_key
        let successors: Vec<String> = store.entries.iter()
            .filter(|(_, v)| {
                v.get("previous_key")
                    .and_then(|pk| pk.as_str())
                    .is_some_and(|pk| pk == old_key)
                && v.get("deleted_at").is_none()  // still alive
            })
            .map(|(k, _)| k.clone())
            .collect();

        // 3. Tombstone losers and redirect their references
        for successor_key in &successors {
            if successor_key != winning_new_key {
                tombstone_entry(store, successor_key, winning_new_key);
                redirect_references(store, successor_key, winning_new_key);
            }
        }

        // 4. Redirect references from old_key to winning_new_key
        redirect_references(store, old_key, winning_new_key);
    }
}
```

This algorithm is fully generic. It doesn't know about projects, areas, or any specific entity type. It operates on keys, tombstones, and the `renamed_to` / `previous_key` metadata fields.

### Rename chains

What if an entity is renamed multiple times? "Website" → "Blog" → "Tech Blog"

The chain of tombstones looks like:
```
project/website  → { renamed_to: "project/blog" }
project/blog     → { renamed_to: "project/tech-blog" }
project/tech-blog → { name: "Tech Blog" }  (live)
```

When repairing references, we follow the chain to the terminal (non-tombstoned) entry. If a reference points to `project/website`, we follow: website → blog → tech-blog. The reference gets updated to `project/tech-blog`.

We should set a maximum chain depth (e.g., 10) to prevent infinite loops from bugs. In practice, entities are rarely renamed more than once or twice.

## Refactoring Impact on tdo

Moving tdo to this model touches several areas. Here's the breakdown:

### Models layer — Significant changes

**`models/store.rs`** — The biggest change. `Store` shifts from `HashMap<Uuid, Entity>` to `HashMap<String, Entity>` where the key is the natural key. The `StoredStore` / `Store` conversion is replaced by the KV format.

```rust
// Before
pub struct Store {
    pub tasks: HashMap<Uuid, Task>,
    pub projects: HashMap<Uuid, Project>,
    pub areas: HashMap<Uuid, Area>,
}

// After
pub struct Store {
    pub version: u32,
    pub tasks: HashMap<String, Task>,      // key: "task/k7m2a3x9"
    pub projects: HashMap<String, Project>, // key: "project/website"
    pub areas: HashMap<String, Area>,       // key: "area/work"
    // next_task_number computed at runtime: max(task_numbers) + 1
}
```

All lookup methods change signature: `get_project(id: Uuid)` becomes `get_project(key: &str)`. For tasks, `get_task_by_number(42)` uses a secondary index `task_number → storage_key` that is rebuilt from entries on load.

**`models/project.rs`, `models/area.rs`** — Remove `id: Uuid` field. Add `impl Entity` with the natural key function.

**`models/task.rs`** — Remove `id: Uuid` field. Foreign key fields change from `project_id: Option<Uuid>` to `project_key: Option<String>` (full key, e.g., `"project/website"`). Same for `area_id` → `area_key`, `parent_task_id` → `parent_task_key`, and `depends_on: Vec<Uuid>` → `depends_on: Vec<String>`.

### Services layer — Moderate changes

**`services/tasks.rs`, `services/projects.rs`, `services/areas.rs`** — Every place that does `store.get_project(project_id)` changes to `store.get_project("project/website")`. Project creation computes the natural key and checks for key existence instead of blindly inserting.

Project/area rename operations use the tombstone + create pattern with reference updates.

### Storage layer — Moderate changes

**`storage/json.rs`** — Serialization/deserialization changes to the new KV format.

**`storage/migrations.rs`** — We need a v8 → v9 migration that:
1. Reads the old `{ tasks: [...], projects: [...], areas: [...] }` format
2. Runs `deduplicate_by_name` one final time to resolve any existing duplicates
3. Computes natural keys for projects and areas (lowercased name)
4. Generates short hash storage keys for tasks (from old UUID + `created_at`)
5. Converts `project_id` / `area_id` UUID references to full `project_key` / `area_key` string references
6. Preserves existing `task_number` values (they stay as local display numbers)
7. Drops `next_task_number` (now computed at runtime as `max(task_numbers) + 1`)
8. Builds the new `{ version: 9, entries: { "project/website": {...}, "task/k7m2a3x9": {...}, ... } }` format
9. Removes all `id: Uuid` fields from entities

This is a one-way migration. We don't need a rollback path because automatic backups (5 versions) already provide a safety net.

### CLI layer — Small changes

**`main.rs`** — Command dispatch mostly stays the same. The `resolve_task_by_id_or_fuzzy` function continues to work (it already does fuzzy matching on names). Project/area resolution by name becomes a direct key lookup instead of a scan — actually faster. Task resolution by number (`tdo done 42`) uses the `task_number → storage_key` index.

### Sync layer — Simplifies dramatically

**`saku-sync/src/conflict.rs`** — The entire file shrinks from ~925 lines to ~60. Everything is deleted except:
- `lww_merge_kv` (the generic merge)
- `reconcile_renames` (generic tombstone/rename handling)
- `compare_modified_at` (unchanged)
- `write_conflict_copy` (for non-JSON files, unchanged)

The functions `deduplicate_by_name`, `build_id_mapping`, `reassign_entity_references`, `fix_duplicate_task_numbers`, `lww_merge_entity_array`, `lww_merge_store_json` are all deleted.

### Effort estimate

| Area | Files | Effort |
| --- | --- | --- |
| `saku-storage` new `Entity` trait + KV store types | 2-3 new files | Medium |
| `tdo` models (remove UUID, add Entity impls, change refs) | 4 files | Medium-High |
| `tdo` services (key-based lookups, rename logic) | 3 files | Medium |
| `tdo` storage (new format, v8→v9 migration) | 2 files | High — migration is the riskiest part |
| `tdo` CLI (reference changes, task number lookup) | 1 file | Small |
| `saku-sync` conflict.rs (rewrite as generic KV merge) | 1 file | Medium — mostly deletion + new tombstone logic |
| `saku-sync` sync_engine.rs (call generic merge) | 1 file | Small |
| Tests | ~10 files | High — rewrite all merge tests, add rename/tombstone tests |

Overall: **a significant refactor** that touches most files. The highest-risk pieces are the v8→v9 migration (transforms real user data) and the task number partitioning (changes how numbers are allocated). Both should have extensive test coverage before shipping.

## Pros and Cons

### Pros

| Benefit | Details |
| --- | --- |
| **Duplicates are impossible** | Same natural key = same entry. No dedup pass needed, ever. |
| **Sync becomes generic** | One `lww_merge_kv` function works for any tool. No entity-type-specific merge code. ~925 lines → ~60 lines. |
| **Local writes enforce uniqueness** | `Store::add_project("Website")` returns the existing project if one exists, instead of silently creating a duplicate. |
| **Simpler mental model** | For named entities: "the key is the name." For tasks: short hash key for storage, sequential number for display. No UUIDs. |
| **New tools get it for free** | `jrn`, `dcs`, `ctx` just implement `Entity` with their natural key function. No custom merge code in the sync layer. |
| **References are human-readable** | `project_key: "project/website"` is instantly debuggable. Reading the JSON store makes sense without cross-referencing UUIDs. |
| **Task number stays sequential** | Each device allocates 1, 2, 3... locally. No jarring gaps. Muscle memory preserved. |
| **Foundation for multi-platform** | A KV store maps naturally to SQLite (iOS), IndexedDB (web), and remote APIs. The merge algorithm is ~30 lines in any language. |

### Cons

| Cost | Details | Mitigation |
| --- | --- | --- |
| **Renames are more complex** | Changing a project name changes its key, requiring tombstone + create + reference updates. | The storage layer handles this automatically via `rename()` API + the schema's `references` list. Application code is a single call. |
| **Migration risk** | The v8 → v9 migration transforms the entire on-disk format. A bug could corrupt data. | Automatic backups already exist (5 versions kept). Extensive test coverage on the migration. Manual backup before upgrading. |
| **Task number mismatch across devices** | The same task can have different display numbers on different devices (your local tasks keep their numbers, incoming tasks get the next available). | Acceptable in single-user multi-device use — you work on one device at a time. The storage key is the true identity for sync. |
| **Natural key must be stable** | If the natural key function changes (e.g., project uniqueness should include area), all existing keys are invalid. | Natural key functions should be simple and stable. We version them if needed. |
| **Tombstone accumulation** | Deleted/renamed entities leave tombstones that grow the store over time. | Tombstones are tiny (key + timestamps). GC after all devices sync. Negligible for our data volumes. |
| **Concurrent rename complexity** | Two devices renaming the same entity to different names requires post-merge reconciliation. | The reconciliation algorithm is generic and well-defined. Edge case is rare in single-user multi-device usage. |

### Comparison with keeping the current approach

| Aspect | Current (UUID + dedup) | Proposed (Natural Key KV) |
| --- | --- | --- |
| Identity mechanism | UUID (opaque, generated) | Natural key (meaningful, deterministic) |
| Uniqueness enforcement | After-the-fact during sync | By construction, at write time |
| Sync merge complexity | ~925 lines, entity-type-specific | ~60 lines, fully generic |
| Adding a new tool | Add dedup logic to `conflict.rs` | Implement `Entity` trait, done |
| Rename handling | Implicit (mutate in place) | Explicit (tombstone + create + forwarding) |
| Foreign references | UUID-based (opaque) | Full key (human-readable, self-describing) |
| Task numbers | Sequential, unstable on sync | Sequential per device, local numbers stable |
| Multi-platform readiness | Poor (sync logic is Rust-only, complex) | Good (merge is ~30 lines in any language) |

## Beyond JSON: Multi-Platform Vision

The natural-key KV model opens a path to running saku's storage on platforms beyond the CLI. Here's how.

### The KV store as an abstraction layer

Today our storage is a JSON file. But the KV model is an abstraction — we can swap the backend without changing the application logic:

```
                    ┌──────────────────┐
                    │   Entity Trait   │
                    │  + Natural Key   │
                    └────────┬─────────┘
                             │
                    ┌────────▼─────────┐
                    │    KV Store API   │
                    │  get / put / del  │
                    │  list_by_prefix   │
                    └────────┬─────────┘
                             │
          ┌──────────────────┼──────────────────┐
          │                  │                  │
  ┌───────▼───────┐  ┌──────▼───────┐  ┌───────▼──────┐
  │  JSON File    │  │   SQLite     │  │  IndexedDB   │
  │  (CLI, now)   │  │  (iOS app)   │  │  (Web app)   │
  └───────────────┘  └──────────────┘  └──────────────┘
```

### iOS companion app

An iOS app for tdo would use a SQLite-backed KV store:

```sql
CREATE TABLE entries (
    key         TEXT PRIMARY KEY,   -- "project/website"
    value       TEXT NOT NULL,      -- JSON blob
    modified_at TEXT NOT NULL,      -- HybridTimestamp as JSON
    deleted_at  TEXT                -- soft-delete timestamp
);
```

The sync protocol stays the same:
1. Local writes go to SQLite
2. Sync reads the remote encrypted store, decrypts, merges using `lww_merge_kv`
3. Push the merged result back

The merge logic is ~30 lines and trivially portable to Swift. No Rust-specific dedup code to rewrite.

**The agent workflow works like this:**

```
Phone (iOS)                          PC (CLI)
─────────                            ────────
User adds task "Deploy staging"
  → SQLite: put("task/r2d7x1", {...})
  → task_number: 5 (on phone)
  → Sync pushes to remote
                                     Agent runs `tdo sync`
                                       → Pulls task "Deploy staging"
                                       → Assigns local task_number: 47
                                     Agent runs `tdo start 47`
                                       → Works on it
                                     Agent runs `tdo done 47`
                                       → Sync pushes completion
User opens app
  → Sees task #5 completed ✓
```

The task has different display numbers on each device (#5 on phone, #47 on CLI), but the storage key `task/r2d7x1` is the same everywhere. Sync works on keys, so both devices see the completion.

### Web app

A web client would use IndexedDB with the same KV structure:

```javascript
// Same merge logic, in JavaScript
function lwwMergeKv(local, remote) {
  const merged = new Map(local.entries);
  for (const [key, remoteValue] of remote.entries) {
    const localValue = merged.get(key);
    if (!localValue || compareModifiedAt(remoteValue, localValue) > 0) {
      merged.set(key, remoteValue);
    }
  }
  return merged;
}
```

The simplicity of the merge algorithm is the point. A UUID-based merge with name dedup and reference reassignment would be much harder to port correctly.

### What we'd need to build

To support multi-platform, we'd layer the work:

**Phase 1 (this RFC):** Natural-key KV store in `saku-storage`, tdo migration. CLI-only.

**Phase 2:** Extract the KV store API as a protocol (not just a Rust trait). Define the JSON wire format for entries, the merge algorithm specification, and the sync handshake. This lets non-Rust clients implement the protocol.

**Phase 3:** SQLite backend for `saku-storage`. This is useful even for CLI — large stores would benefit from indexed queries instead of loading everything into memory.

**Phase 4:** iOS / web client using the protocol from Phase 2 and the SQLite/IndexedDB backend.

> Phase 1 is the foundation. Phases 2-4 are enabled by it but not required. We can ship Phase 1 and decide later whether to pursue the others.

### What this changes about the sync server

The current sync server (`saku-server`) acts as a dumb storage relay — it holds encrypted blobs and serves presigned S3 URLs. It never looks at the data. This architecture stays the same for multi-platform sync.

However, for real-time collaboration (e.g., phone and PC both open), we'd eventually want the server to support **push notifications** or **change feeds** so clients know when to sync. This is future work and orthogonal to the KV store change.

## Decisions Made

These questions were raised during drafting and have been resolved:

1. **UUIDs are removed entirely.** They have no place in this architecture. No UUID fields, no UUID indexes, no UUID references. Tasks use short generated hashes as storage keys; named entities use their name.

2. **No transition period.** This is a single-user project with no third parties holding UUID references. The v8 → v9 migration is a clean break.

3. **Foreign references use the full storage key.** `project_key: "project/website"`, not `project_key: "website"`. Full keys are self-describing and unambiguous regardless of context.

4. **Migration handles existing duplicates.** The v8 → v9 migration runs `deduplicate_by_name` one final time to resolve any pre-existing duplicate projects or areas. After migration, the KV model prevents duplicates by construction.

5. **Concurrent renames resolved via tombstone reconciliation.** The `renamed_to` / `previous_key` mechanism with post-merge reconciliation handles all rename conflict scenarios generically. See the tombstone deep dive above.

6. **Task numbers stay sequential, storage keys are separate.** Device-partitioned number ranges (1-999, 1000-1999...) were rejected because the gaps are jarring and break muscle memory. Instead, tasks use a short generated hash as the storage key and keep sequential task numbers as a local display property. Each device allocates 1, 2, 3... independently. On sync, local numbers never change; incoming tasks get the next available number. Numbers may differ across devices, but the storage key is the true identity.

7. **Tombstone GC is time-based, 30 days.** We garbage-collect tombstones older than 30 days. This is safe because in practice all devices sync within that window. If a device hasn't synced in 30 days and then syncs, a deleted entity might resurrect — but this is an acceptable edge case for a personal productivity tool. The alternative (waiting for all devices to confirm sync) is complex and fragile (what if a device is retired?).

8. **Natural key migrations use the existing migration system.** If a natural key function changes in the future (e.g., scoping projects per-area), we handle it as a schema migration: the new version writes to the new key format, migrating all entries. This is a breaking change that requires a version bump, same as any other schema migration. The migration infrastructure must be strong enough to handle key rewrites — we should verify this during Phase 1 implementation.

9. **No rename history beyond `previous_key`.** A single `previous_key` hop is sufficient. Debugging and auditing are not current concerns. Rename chains are followed automatically during sync, but we don't persist the full history.

10. **Incoming tasks numbered by `created_at` order.** When syncing tasks from another device, they're assigned local task numbers in `created_at` order. This preserves the creation sequence from the other device, so the user sees them in the order they were originally created.

11. **No metadata section in the KV store.** The store is just `version` + `entries`. Values like `next_task_number` are computed at runtime (`max(task_numbers) + 1`), not stored or synced. This eliminates the need for metadata merge strategies entirely — the only thing that needs merging is the entries map, which is fully generic LWW.

## Open Questions

All major design questions have been resolved. No blocking open questions remain for Phase 1 implementation.

## References

- `crates/saku-storage/src/timestamp.rs` — HybridTimestamp definition
- `crates/saku-storage/src/lib.rs` — Current storage public API
- `crates/tdo/src/models/store.rs` — Current StoredStore and Store
- `crates/tdo/src/models/project.rs` — Project struct (name field)
- `crates/tdo/src/models/area.rs` — Area struct (name field)
- `crates/tdo/src/models/task.rs` — Task struct (UUID + project_id + area_id)
- `crates/saku-sync/src/conflict.rs` — Current LWW merge + dedup logic (~925 lines)
- `crates/saku-sync/src/sync_engine.rs` — Sync loop that calls `lww_merge_store_json`
- `documentation/architecture.md` — Overall architecture and storage strategy
- `documentation/sync-architecture.md` — Sync system design
