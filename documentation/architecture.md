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

The default store is `~/.local/share/tdo/store.sqlite3` (SQLite schema v4); `TDO_DATA_DIR` overrides its directory. Portable JSON uses version 9:

```json
{"version": 9, "entries": {}}
```

Entities use natural-key storage keys such as `project/website`, `area/work`, and `task/{suffix}`. New task suffixes use UUID v4. Tasks also have user-facing numeric identifiers. The in-memory store uses maps and a task-number index.

Mutating commands use an immediate SQLite transaction for the whole read-modify-write session. Entries, the revisioned outbox, and the persisted hybrid clock are committed together. Task editor sessions reserve timestamps without holding a writer lock and reject stale saves.

Existing v9 JSON ledgers require explicit `tdo migrate` or `tdo import`; the source is retained. `tdo export` produces portable v9 JSON without overwriting an existing file. Deletion uses soft tombstones, and renames use tombstone/new-entry pairs with timestamped reference updates.

## Current Sync

The default `sync` feature enables server sync and OS keychain integration. Network sync is explicit through `tdo sync run`; ordinary local commands do not sync before or after execution and do not use a `TDO_SYNC_DIR` fallback. `TDO_NO_SYNC=1` disables explicit sync.

The server path uses encrypted per-entry HTTP sync, with its cursor and revisioned outbox stored in the task database. Guarded KV v2 writes use the last completed pull cookie as `base_cookie`: the server atomically checks every submitted key against that watermark before writing the batch. A 409 causes a pull, merge, and regenerated push, with at most three retries. Push responses never advance the pull cursor. The client uses last-writer-wins merging with deterministic equal-clock tie-breaks, rename reconciliation, and reference repair; it does not preserve all conflicts. Undecryptable entries fail closed. SQLite schema v4 and portable v9 JSON are unchanged by the protocol update. The JSON storage and whole-file sync engine remain library-only for historical tests.

On divergent pulls or merge repairs, local recovery history preserves full JSON values for the local, remote, and resulting versions when they differ. It is plaintext like the task database, never synced, and neither automatically pruned nor restored. The recorded source is the first provenance, not a winner/loser classification. History cannot recover historical unseen server overwrites or classify concurrent edits without a common base.

Guarded KV v2 uses startup-bound cookies (`<server-instance>:<sequence>`). The server generates a new random instance ID outside its database at every startup, so normal restarts and stop/restore/start recovery invalidate old cookies. HTTP 410 makes the client atomically requeue all local entries and clear its cursor, then fully pull and merge before guarded pushes. Missing remote entries do not delete local entries; tombstones and divergent history are preserved. Stale writes return 409; stale-write and cursor-recovery retries share a limit of three retries. V1 writes return 428, and new clients refuse old servers without fallback. SQLite schema v4 and v9 JSON are unchanged.

Recovery supports one running server instance. Stop the server before replacing its SQLite database, then start it normally; no manual epoch rotation is needed. Every restart causes one full reconciliation, even when the database has not changed. Editing or restoring a live SQLite database in place is unsupported. Alternating requests across independent server instances can exhaust the bounded recovery retries. Recovery restores only values still present on a client or server; it cannot reconstruct a version absent from every copy. Deploy the matching server and clients together before live rollout.

Use `tdo history list --json` or `tdo history list --key task/...`, `tdo history show 1`, and `tdo history export recovery.json` to inspect or export it. The version 1 history archive is not importable with `tdo import`; ordinary v9 `tdo export` excludes history. Export both to preserve portable contents, not as a full operational backup.

[Sync setup](sync-setup.md) covers the current KV deployment and recovery procedure. [Sync architecture](sync-architecture.md) is legacy research. The [distributed KV RFC](rfc-distributed-kv-sync.md) records the per-entry design.

## Development

Linux builds bundle the D-Bus client library through keyring's vendored feature, so libdbus development headers and custom pkg-config metadata are not required. A C compiler is still needed for bundled SQLite and D-Bus. Runtime sync credentials still use the session's Secret Service keychain (for example, GNOME Keyring or KWallet); bundling the client library does not provide that service. Local task commands work without a keychain service.

Run commands from the repository root:

```bash
cargo build -p saku-tdo
cargo run -p saku-tdo -- view today
cargo test -p saku-tdo --no-default-features
cargo test --workspace --no-default-features
```

The no-default-feature test commands avoid server/keychain dependencies. A full-feature build uses `cargo build --workspace`; release binaries can be built with `cargo build --release --workspace`.

See [task testing](tdo/testing.md) and the project rules in `CLAUDE.md` for additional conventions.
