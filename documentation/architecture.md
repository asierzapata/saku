# Saku Architecture

This document describes the current task-focused workspace. See [PHILOSOPHY.md](PHILOSOPHY.md) for product boundaries and future interface goals.

## Current Workspace

| Crate | Role |
|---|---|
| `saku-tdo` | Task CLI and task domain library; binary `tdo`. |
| `saku-storage` | Entity keys, hybrid timestamps, KV merging, dirty tracking, and file I/O helpers. |
| `saku-crypto` | Client-side encryption and optional OS keychain integration. |
| `saku-sync` | Per-entry server sync and the retained legacy whole-file sync engine. |
| `saku-server` | Authentication, encrypted KV endpoints, and legacy S3 coordination. |

`mdr` (terminal Markdown reader) and `sch` (local command scheduler) are planned products, not workspace members. Development is focused on `tdo` first.

## Task CLI Layers

- `crates/tdo/src/main.rs`: command parsing, dispatch, view selection, and sync integration.
- `crates/tdo/src/services/`: task, project, and area operations.
- `crates/tdo/src/models/`: tasks, projects, areas, recurrence, and store types.
- `crates/tdo/src/storage/`: persistence and schema migrations.
- `crates/tdo/src/ui.rs`: terminal rendering.
- `crates/tdo/src/output.rs`: structured output.

The existing CLI is shared by humans, scripts, and agents. Bare invocation currently renders today's tasks; a TUI is a future change. Read commands support structured output where applicable, including JSON, CSV, and TOON.

## Current Task Storage

The default store is `~/.local/share/tdo/store.json`; `TDO_DATA_DIR` overrides its directory. The current on-disk format is version 9:

```json
{"version": 9, "entries": {}}
```

Entities use natural-key storage keys such as `project/website`, `area/work`, and `task/k7m2a3x9`. Tasks also have user-facing numeric identifiers. The in-memory store uses maps and a task-number index.

Individual saves take a file lock, retain up to five backup files in a sibling `backups/` directory, and replace the store atomically. This does not make the complete read-modify-write operation transactional.

Schema migrations run when older stores are loaded. Deletion uses tombstones, and renames use tombstone/new-entry pairs with reference repair.

## Current Sync

The default `sync` feature enables server sync and OS keychain integration. The CLI attempts sync before loading task data and after mutating commands. `TDO_NO_SYNC=1` disables these attempts; `TDO_SYNC_DIR` selects the legacy local-filesystem fallback.

The server path uses encrypted per-entry HTTP sync and a dirty-tracker sidecar. The server stores those entries in SQLite. The legacy whole-file engine, Merkle comparison, and S3 endpoints remain available in the infrastructure; their replacement or retirement is not part of the product cleanup.

[Sync setup](sync-setup.md) and [sync architecture](sync-architecture.md) include legacy whole-file material. The [distributed KV RFC](rfc-distributed-kv-sync.md) records the per-entry design.

The cleanup does not change the storage format, sync behavior, keychain, deployed services, or user data.

## Development

Run commands from the repository root:

```bash
cargo build -p saku-tdo
cargo run -p saku-tdo -- view today
cargo test -p saku-tdo --no-default-features
cargo test --workspace --no-default-features
```

The no-default-feature test commands avoid server/keychain dependencies. A full-feature build uses `cargo build --workspace`; release binaries can be built with `cargo build --release --workspace`.

See [task testing](tdo/testing.md) and the project rules in `CLAUDE.md` for additional conventions.
