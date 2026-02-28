# Plan: Natural-Key-Based KV Store for saku-storage

## Goal

Replace the current UUID-based monolithic JSON store with a natural-key-based key-value store. Projects and areas use their lowercased name as the storage key. Tasks use a short hash derived from `(device_id, creation_timestamp)`. UUIDs are removed entirely. The sync layer becomes fully generic (~60 lines instead of ~925). On-disk format changes from entity arrays to a flat `entries` map (v8 → v9 migration).

Reference RFC: `documentation/rfc-natural-key-kv-store.md`

## Context

### Key files

- `crates/saku-storage/src/lib.rs` — Storage public API (currently: device, io, timestamp)
- `crates/saku-storage/src/timestamp.rs` — `HybridTimestamp` definition
- `crates/tdo/src/models/store.rs` — `StoredStore` (on-disk), `Store` (in-memory), all lookup methods
- `crates/tdo/src/models/task.rs` — `Task` struct with `id: Uuid`, `project_id`, `area_id`, `parent_task_id`, `depends_on`
- `crates/tdo/src/models/project.rs` — `Project` struct with `id: Uuid`, `area_id`
- `crates/tdo/src/models/area.rs` — `Area` struct with `id: Uuid`
- `crates/tdo/src/services/tasks.rs` — Task CRUD, uses `store.get_project(uuid)`
- `crates/tdo/src/services/projects.rs` — Project CRUD, cascade delete
- `crates/tdo/src/services/areas.rs` — Area CRUD, cascade delete
- `crates/tdo/src/storage/json.rs` — `JsonFileStorage` load/save, version detection
- `crates/tdo/src/storage/migrations.rs` — v1→v8 migrations, `CURRENT_VERSION = 8`
- `crates/saku-sync/src/conflict.rs` — `lww_merge_store_json`, `deduplicate_by_name`, `reassign_entity_references` (~925 lines)
- `crates/saku-sync/src/sync_engine.rs` — Calls `lww_merge_store_json` during pull
- `crates/tdo/src/main.rs` — CLI dispatch, `resolve_task_by_id_or_fuzzy`, project/area resolution
- `crates/tdo/src/ui.rs` — Terminal rendering (references entities by store lookups)
- `crates/tdo/src/output.rs` — JSON/CSV output structs

### Patterns to follow

- Migrations are sequential functions in `migrations.rs`, each bumping version by 1
- `StoredStore` ↔ `Store` conversion is done via `Store::from_stored()` / `store.to_stored()`
- Sync operates on `serde_json::Value`, not typed structs
- Tests: `cargo test -p saku-tdo --no-default-features` (avoids keychain/network)
- Commit format: `(scope) lowercase imperative description`

### Agreed contracts

**Entity trait (saku-storage):**
```rust
pub trait Entity: Serialize + DeserializeOwned {
    fn entity_type() -> &'static str;
    fn natural_key(&self) -> String;
    fn storage_key(&self) -> String {
        format!("{}/{}", Self::entity_type(), self.natural_key())
    }
}
```

**KvStore on-disk format:**
```rust
pub struct KvStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,
}
```

**Model changes:**
- Task: Remove `id: Uuid`, add `storage_key_suffix: String`. `project_id` → `project_key: Option<String>`, `area_id` → `area_key: Option<String>`, `parent_task_id` → `parent_task_key: Option<String>`, `depends_on: Vec<Uuid>` → `depends_on: Vec<String>`
- Project: Remove `id: Uuid`, `area_id` → `area_key: Option<String>`
- Area: Remove `id: Uuid`

**Store in-memory:**
- `HashMap<String, Task>` (key: `"task/k7m2a3x9"`)
- `HashMap<String, Project>` (key: `"project/website"`)
- `HashMap<String, Area>` (key: `"area/work"`)
- `next_task_number` removed (computed as `max(task_numbers) + 1`)
- Secondary index: `HashMap<u64, String>` for `task_number → storage_key`

**Sync (conflict.rs):**
- Delete: `deduplicate_by_name`, `build_id_mapping`, `reassign_entity_references`, `fix_duplicate_task_numbers`, `lww_merge_entity_array`, `lww_merge_store_json`
- Add: `lww_merge_kv`, `reconcile_renames`, `repair_references`

**Migration:** v8 → v9 (one-way, `CURRENT_VERSION` bumped to 9)

---

## Phase 1: Entity trait and KV store types in `saku-storage`

### Description

Add the foundational `Entity` trait and `KvStore` type to `saku-storage`. This phase adds new code only — no existing code is modified. The trait and types will be used by subsequent phases.

### To-do

- [x] Create `crates/saku-storage/src/entity.rs` with:
  - `Entity` trait: `entity_type() -> &'static str`, `natural_key(&self) -> String`, `storage_key(&self) -> String` (default impl)
  - `EntitySchema` struct: `entity_type: &'static str`, `references: Vec<(&'static str, &'static str)>`
- [x] Create `crates/saku-storage/src/kv_store.rs` with:
  - `KvStore` struct: `version: u32`, `entries: HashMap<String, serde_json::Value>`
  - `lww_merge_kv(local: &KvStore, remote: &KvStore) -> KvStore` — generic LWW merge on entries
  - `compare_modified_at(a: &Value, b: &Value) -> Ordering` — extracts and compares `modified_at` from JSON values
  - `reconcile_renames(store: &mut KvStore)` — tombstone-based rename reconciliation
  - `repair_references(store: &mut KvStore, schemas: &[EntitySchema])` — follows `renamed_to` chains and updates dangling references
  - Tombstone GC function: `gc_tombstones(store: &mut KvStore, max_age_days: u32)`
- [x] Create `crates/saku-storage/src/key_gen.rs` with:
  - `generate_task_key(device_id: &str, creation_ms: i64) -> String` — SHA-256 hash, first 5 bytes, base-36 encoded
  - `base36_encode(num: u64) -> String` helper
- [x] Export all new modules from `crates/saku-storage/src/lib.rs`
- [x] Add `sha2` dependency to `crates/saku-storage/Cargo.toml`
- [x] Write unit tests for:
  - `lww_merge_kv`: same key LWW winner, new keys from both sides, tombstone wins over live
  - `reconcile_renames`: single rename, concurrent rename, rename chain (max depth)
  - `repair_references`: dangling reference fixed, chain followed
  - `gc_tombstones`: old tombstones removed, recent ones kept
  - `generate_task_key`: deterministic output, different inputs produce different keys
  - `base36_encode`: known values

### Verification

```bash
cargo test -p saku-storage
cargo build -p saku-storage
```

All new tests pass. No existing tests broken.

---

## Phase 2: Update tdo models (Task, Project, Area, Store)

### Description

Replace UUID-based identity with natural keys in all tdo model structs. Update `Store` to use `HashMap<String, T>` and compute `next_task_number` at runtime. Add `Entity` trait implementations. This is the largest model change — all foreign references change type.

### To-do

- [x] Update `crates/tdo/src/models/area.rs`:
  - Remove `id: Uuid` field
  - Implement `Entity` trait: `entity_type() = "area"`, `natural_key() = name.to_lowercase()`
  - Update `new()` constructor to not generate UUID
  - Keep `deleted_at`, `modified_at` fields unchanged
- [x] Update `crates/tdo/src/models/project.rs`:
  - Remove `id: Uuid` field
  - Change `area_id: Option<Uuid>` → `area_key: Option<String>`
  - Implement `Entity` trait: `entity_type() = "project"`, `natural_key() = name.to_lowercase()`
  - Update `new()` constructor
  - Keep `deleted_at`, `modified_at`, `completed_at`, `created_at`, `deadline`, `notes` unchanged
- [x] Update `crates/tdo/src/models/task.rs`:
  - Remove `id: Uuid` field
  - Add `storage_key_suffix: String` (the hash portion, e.g., `"k7m2a3x9"`)
  - Change `project_id: Option<Uuid>` → `project_key: Option<String>`
  - Change `area_id: Option<Uuid>` → `area_key: Option<String>`
  - Change `parent_task_id: Option<Uuid>` → `parent_task_key: Option<String>`
  - Change `depends_on: Vec<Uuid>` → `depends_on: Vec<String>`
  - Implement `Entity` trait: `entity_type() = "task"`, `natural_key() = storage_key_suffix.clone()`
  - Update `new()` to call `generate_task_key(device_id, now_ms)` instead of `Uuid::new_v4()`
  - Update `order_tasks()` if it uses `id` for tiebreaking → use `storage_key_suffix`
- [x] Update `crates/tdo/src/models/store.rs`:
  - Change `StoredStore` to new KV format: `version: u32`, `entries: HashMap<String, serde_json::Value>`
  - Change `Store` fields: `tasks: HashMap<String, Task>`, `projects: HashMap<String, Project>`, `areas: HashMap<String, Area>` (keys are full storage keys like `"task/k7m2a3x9"`)
  - Remove `next_task_number` field — compute as `max(task_numbers) + 1` via method
  - Add `task_number_index: HashMap<u64, String>` — built on load
  - Update `from_stored()` to deserialize from KV entries format and build indexes
  - Update `to_stored()` to serialize back to KV entries format
  - Update `add_task()` to compute `task_number` at runtime
  - Update `add_project()` to check key existence (enforce uniqueness)
  - Update `add_area()` to check key existence (enforce uniqueness)
  - Update `get_task_by_number()` to use the secondary index (O(1) instead of linear scan)
  - Update all `get_*` methods to take `&str` key instead of `Uuid`
  - Add `rename_project(old_name, new_name)` and `rename_area(old_name, new_name)` methods with tombstone + create + reference update logic
- [x] Remove `uuid` dependency from `crates/tdo/Cargo.toml` (if no longer used anywhere)
- [x] Add `saku-storage` dependency updates if needed (for `Entity`, `generate_task_key`)
- [x] Fix all compiler errors within `models/` module
- [x] Update existing model unit tests to work with new types

### Verification

```bash
cargo build -p saku-tdo --no-default-features
cargo test -p saku-tdo --no-default-features -- models
```

Model code compiles. Model unit tests pass. (Services, CLI, and integration tests will be broken — that's expected, fixed in later phases.)

---

## Phase 3: Update tdo services layer

### Description

Update all service functions (`tasks.rs`, `projects.rs`, `areas.rs`, `task_editor.rs`) to use string keys instead of UUIDs. Update project/area creation to use natural key lookups. Add rename operations.

### To-do

- [x] Update `crates/tdo/src/services/tasks.rs`:
  - All `store.tasks.get(&uuid)` → `store.tasks.get(storage_key)`
  - All `task.project_id` → `task.project_key`, `task.area_id` → `task.area_key`
  - Task creation: call `generate_task_key()`, compute `task_number = store.next_task_number()`
  - Dependency resolution: `depends_on` now holds storage keys
  - Subtask lookup: `parent_task_key` instead of `parent_task_id`
- [x] Update `crates/tdo/src/services/projects.rs`:
  - `create_project()`: compute natural key, check existence, return existing if duplicate
  - `delete_project()`: cascade uses `task.project_key == project_storage_key`
  - `restore_project()`: lookup by key
  - Add `rename_project()` service function using store's rename method
- [x] Update `crates/tdo/src/services/areas.rs`:
  - `create_area()`: compute natural key, check existence, return existing if duplicate
  - `delete_area()`: cascade uses `project.area_key` and `task.area_key`
  - `restore_area()`: lookup by key
  - Add `rename_area()` service function
- [x] Update `crates/tdo/src/services/task_editor.rs`:
  - `serialize_task_for_edit()` and `parse_edited_task()`: use key-based references instead of UUIDs
- [x] Update service unit tests (`projects.rs`, `areas.rs`, `task_editor.rs` tests)

### Verification

```bash
cargo build -p saku-tdo --no-default-features
cargo test -p saku-tdo --no-default-features -- services
```

Services compile and their unit tests pass.

---

## Phase 4: Update tdo CLI layer (main.rs, ui.rs, output.rs)

### Description

Update the CLI command dispatch, resolution functions, rendering, and output serialization to work with natural keys. This makes the binary compile and run end-to-end.

### To-do

- [x] Update `crates/tdo/src/main.rs`:
  - `resolve_task_by_id_or_fuzzy()`: task resolution by number uses `store.get_task_by_number()` (already index-based after Phase 2), fuzzy match on title unchanged
  - Project/area resolution by name: direct key lookup `store.projects.get(&format!("project/{}", name.to_lowercase()))` instead of scanning
  - All command handlers: replace `Uuid` references with `String` keys
  - `add` command: project/area lookup by name → key
  - `done` command: task lookup by number → key
  - `depend` command: dependency keys
  - `move` command: project/area key references
  - `view` commands: iterate by key
- [x] Update `crates/tdo/src/ui.rs`:
  - `render_task_line()`: resolve `project_key` / `area_key` from store
  - `render_task_detail_view()`: display project/area names from keys
  - All other render functions that reference entities by ID
- [x] Update `crates/tdo/src/output.rs`:
  - `TaskOutput`: change `id` field (consider keeping a string `key` field for JSON output)
  - `ProjectOutput`, `AreaOutput`: remove UUID, use name as identifier
  - References in output structs: use names/keys instead of UUIDs
- [x] Remove `uuid` crate import from `main.rs` if no longer needed
- [x] Fix all remaining compiler errors in the `tdo` crate

### Verification

```bash
cargo build -p saku-tdo --no-default-features
cargo test -p saku-tdo --no-default-features
```

The full binary compiles. Existing unit tests pass (integration tests may still fail due to on-disk format changes — that's Phase 5).

---

## Phase 5: Storage migration v8 → v9

### Description

Implement the one-way migration from the old array-based format to the new KV entries format. This is the highest-risk phase — it transforms real user data. Bump `CURRENT_VERSION` to 9.

### To-do

- [x] Update `crates/tdo/src/storage/migrations.rs`:
  - Add `migrate_v8_to_v9(value: &mut Value)` function
  - Bump `CURRENT_VERSION` to 9
  - Register `migrate_v8_to_v9` in `apply_migrations()`
- [x] Update `crates/tdo/src/storage/json.rs`:
  - Update deserialization to handle KV format
- [x] Write extensive migration tests:
  - Simple store with projects, areas, tasks — verify all keys correct
  - Store with tasks referencing projects/areas — verify FK conversion
  - Store with subtasks (`parent_task_id`) — verify conversion
  - Store with dependencies (`depends_on`) — verify conversion
  - Store with deleted entities (`deleted_at` set) — verify preserved
  - Store with no tasks (empty arrays) — edge case
  - Round-trip: migrate → load → save → load — data identical
  - Full v1→v9 chain migration

### Verification

```bash
cargo test -p saku-tdo --no-default-features -- migrations
cargo test -p saku-tdo --no-default-features -- storage
```

All migration tests pass. Loading a migrated store produces valid `Store` objects with correct keys and references.

---

## Phase 6: Update sync layer (saku-sync)

### Description

Replace the entity-type-specific merge in `conflict.rs` with the generic KV merge from `saku-storage`. Update `sync_engine.rs` to call the new merge function. Delete ~865 lines of dedup code.

### To-do

- [x] Update `crates/saku-sync/src/conflict.rs`:
  - Deleted all UUID-based merge logic (~250 lines)
  - Added thin `merge_store_json` wrapper using saku-storage KV primitives
  - Kept `write_conflict_copy`
- [x] Update `crates/saku-sync/src/sync_engine.rs`:
  - Changed call from `lww_merge_store_json` to `merge_store_json`
- [x] Write new merge tests (LWW, task numbers, project/area, rename repair)
- [x] Update integration tests to v9 KV format

### Verification

```bash
cargo test -p saku-sync
cargo test -p saku-tdo --no-default-features
```

All sync tests pass. Integration tests pass with new format.

---

## Phase 7: Update CLI integration tests

### Description

Update all `tdo` integration tests to work with the new on-disk format and key-based references. These tests use `assert_cmd` with temp directories.

### To-do

- [x] Update `crates/tdo/tests/task_commands.rs` — all 23 tests pass
- [x] Update `crates/tdo/tests/project_commands.rs` — all 9 tests pass
- [x] Update `crates/tdo/tests/area_commands.rs` — all 9 tests pass
- [x] Update `crates/tdo/tests/edit_commands.rs` — all 13 tests pass (rename already tested)
- [x] Update `crates/tdo/tests/recurring_tasks.rs` — all 14 tests pass
- [x] Update `crates/tdo/tests/show_commands.rs` — all 23 tests pass
- [x] Update `crates/tdo/tests/view_commands.rs` — all 28 tests pass
- [x] Update `crates/tdo/tests/workflows.rs` — all 3 tests pass

### Verification

```bash
cargo test -p saku-tdo --no-default-features
```

All integration tests pass. The full test suite is green.

---

## Documentation to Update

- [x] Update `documentation/rfc-natural-key-kv-store.md` — marked status as "Implemented"
- [x] Update `documentation/architecture.md` — updated IDs section, replaced uuid dep
- [x] Update `documentation/sync-architecture.md` — updated merge description for KV store
- [x] Update `CLAUDE.md` — added "Storage model (v9)" section

---

## Next Step

**All phases complete.** The natural-key KV store migration is fully implemented across all crates. All 190 tests pass (157 tdo + 33 sync).
