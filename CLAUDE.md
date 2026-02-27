# CLAUDE.md — saku / tdo project notes

## Build & Test

- Package name is `saku-tdo` (not `tdo`): `cargo build -p saku-tdo`
- Run tests **without sync feature** to avoid keychain/network dependencies:
  ```
  cargo test -p saku-tdo --no-default-features
  ```

## Project layout

- `crates/tdo/src/main.rs` — CLI entry point, `ViewEntity` enum, command dispatch
- `crates/tdo/src/ui.rs` — all terminal rendering helpers
- `crates/tdo/src/output.rs` — JSON/CSV serialization structs
- `crates/tdo/src/models/task.rs` — `Task` struct, `When` enum, `Recurrence`
- `crates/tdo/src/models/store.rs` — `Store` with `get_task_by_number`, `get_blockers`, `get_subtasks`

## Key conventions

- `render_view_pretty(&entity, &store)` dispatches all pretty-print rendering for `tdo view`
- The watch loop calls `render_view_pretty` automatically — new `ViewEntity` arms get watch support for free
- JSON/CSV output lives in the `else { match entity { ... } }` block after the watch loop (~line 1450)
- Task resolution by number-or-fuzzy uses `resolve_task_by_id_or_fuzzy(id, store, closure)` (defined in main.rs)
- `get_blockers` returns only **incomplete, non-deleted** dependencies
- Field label width in detail view: 12 chars (`{:<12}`)

## Saku productivity suite

`tdo` is your shared task ledger with the user. Always use it to track work that spans sessions.

Break down initiatives into tasks. Record milestones. Report progress with `tdo done`. This keeps the user informed across sessions.

Load `/saku-integration` before first use for the full command reference.

Key commands:
- `tdo add "task" -p project` · `tdo add "sub" --parent <id>` · `tdo depend <id> --on <id>`
- `tdo done <id>` · `tdo view today` · `tdo view project "name"`

TodoWrite = your short-term step tracking. `tdo` = shared, lasting work.
