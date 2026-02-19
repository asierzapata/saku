# Testing Strategy

This document describes the two layers of tests in `crates/tdo` and what each layer covers.

---

## Layer 1 — Unit tests (`src/**/*.rs`)

Unit tests live alongside the source code in `#[cfg(test)]` modules inside each service file. They test **business logic in isolation**, using a `MockStorage` stub that holds data in memory and never touches the filesystem.

| File | What is tested |
|------|---------------|
| `src/services/areas.rs` | `create_area`, `delete_area` (including cascade to projects and tasks), `restore_area` |
| `src/services/projects.rs` | `create_project` (with/without area, duplicate detection), `delete_project` (cascade to tasks), `restore_project` |
| `src/services/tasks.rs` | `add_task`, `complete_task`, `delete_task`, `restore_task`, `move_task` — all variants and error paths |
| `src/services/task_editor.rs` | TOML serialization/deserialization of a task for the editor workflow |
| `src/storage/json.rs` | `JsonFileStorage` — save/load round-trip, task-number auto-increment, migration application, backup creation and cleanup |
| `src/storage/migrations.rs` | Version detection, forward-migration logic |

**What they do not cover:** the CLI argument-parsing layer (`main.rs`) and the rendered output (`ui.rs`). A service can return the right data while `main.rs` formats or routes it incorrectly — unit tests will not catch that.

---

## Layer 2 — Integration tests (`tests/*.rs`)

Integration tests compile and run the real `tdo` binary (via `assert_cmd`). Each test gets an isolated `TempDir` injected through the `TDO_DATA_DIR` environment variable, so tests are fully parallel and leave no state behind.

They exercise **the full stack**: argument parsing → service logic → `JsonFileStorage` on the real filesystem → CLI output.

| File | Commands covered |
|------|-----------------|
| `tests/area_commands.rs` | `area new`, `area list`, `area delete` (with cascade), `area view` |
| `tests/project_commands.rs` | `project new`, `project list`, `project delete` (with cascade), `project view` |
| `tests/task_commands.rs` | `add` (inbox, today, someday, scheduled, with project/area/tags), `done`, `delete`, `restore`, `move` |
| `tests/view_commands.rs` | Default view, `today`, `inbox`, `upcoming`, `someday`, `logbook`, `trash`, `all`, `tag list`, `tag view` — both empty states and populated states |

Coverage scope is **happy-path only**. Error paths (unknown project, ambiguous name, etc.) are covered by unit tests where the logic lives. The `edit` command is excluded because it requires an interactive editor.

---

## Rationale for the two-layer split

- **Unit tests** are fast and precise. They pin the exact inputs and outputs of each service function, making regressions easy to localise.
- **Integration tests** catch bugs that unit tests structurally cannot: wrong CLI flag wiring, incorrect output formatting, `main.rs` routing errors, and storage behaviour on a real filesystem. The duplicate-project bug discovered during test implementation (the `ProjectAlreadyExists` check was missing from `create_project`) is an example of something only an integration test surfaced.

Running both layers:

```bash
cargo test -p saku-tdo
```
