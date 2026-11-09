# Working on saku

Make your work easy for the next person or agent to understand, review, and continue. Include the context needed to make a decision, and keep the rest proportional to the task.

## Scope and progress

Follow the user's objective and existing decisions. Read the relevant code and documentation before choosing an approach. Ask when a missing answer would change the result or make an action unsafe, and reuse answers already available in the conversation or the repository.

When you evaluate a design, consider how others will use and change it later, not only how it behaves the first time. If its safety depends on someone remembering a rule, treat the mistake as likely and prefer a design that prevents or detects it, for example with a type, a test, or a clippy lint.

Choose the smallest coherent change that achieves the objective. Avoid unrelated cleanup, new abstractions without a current need, and speculative features. For work with several purposes, describe a short sequence of reviewable changes before implementing, and do not leave an intermediate state broken.

Changes to the on-disk format, the sync protocol, or the CLI output need extra care, because they affect existing user data, other devices running older versions, and agents that parse the output. Say so explicitly when a change touches one of them.

## Communication

Lead with the answer, result, or decision. Use ordinary words and complete sentences. When shorter text is wanted, select less content instead of dropping the words that connect ideas.

For a routine task, a short paragraph is usually enough. Add detail when the reader needs it to assess a risk, reproduce a result, or make a decision. Do not narrate each step, repeat the plan, or list every changed file.

## Code and comments

Use existing names and patterns when they fit. Prefer readable code over explanatory comments. Add a comment for a non-obvious invariant, an external constraint, a compatibility issue, or a decision that a maintainer could otherwise break, and explain the reason in a short, complete sentence. Do not narrate the next statement or leave an implementation diary.

## Build and verification

- Package name is `saku-tdo` (not `tdo`): `cargo build -p saku-tdo`.
- Run tests **without the sync feature** to avoid keychain and network dependencies: `cargo test -p saku-tdo --no-default-features`.
- CI runs `cargo build` and `cargo test` on pushes and PRs to `main`. Run `cargo clippy` for the crates you change.

Run the checks that can catch a plausible failure in the change. Distinguish checks that ran, checks that failed, and checks that did not run. Never claim a test result, a performance gain, or a cause that you did not observe.

## Project layout

- `crates/tdo/src/main.rs` — CLI entry point, `ViewEntity` enum, command dispatch
- `crates/tdo/src/ui.rs` — all terminal rendering helpers
- `crates/tdo/src/output.rs` — JSON/CSV serialization structs
- `crates/tdo/src/models/task.rs` — `Task` struct, `When` enum, `Recurrence`
- `crates/tdo/src/models/store.rs` — `Store` with `get_task_by_number`, `get_blockers`, `get_subtasks`

See `documentation/architecture.md` for conventions shared across all tools, and `documentation/PHILOSOPHY.md` for the design intent.

## Key conventions

- `render_view_pretty(&entity, &store, completion_filter, &view_filters)` dispatches all pretty-print rendering for `tdo view`
- The watch loop calls `render_view_pretty` automatically — new `ViewEntity` arms get watch support for free
- JSON/CSV output lives in the `else { match entity { ... } }` block after `if watch {` in the `View` command handler
- Task resolution by number-or-fuzzy uses `resolve_task_by_id_or_fuzzy(id, store, closure)` (defined in main.rs)
- `get_blockers` returns only **incomplete, non-deleted** dependencies
- Field label width in detail view: 12 chars (`{:<12}`)

## Storage model (v9)

- On-disk format: `{ version: 9, entries: { "project/website": {...}, ... } }` — flat KV map
- Entities use natural-key storage keys: `project/{name}`, `area/{name}`, `task/{hash}`
- No UUIDs — tasks use `storage_key_suffix` (hash of device_id + timestamp)
- `Entity` trait in `saku-storage`: `entity_type()`, `natural_key()`, `storage_key()`
- `Store` uses `HashMap<String, T>` with `task_number_index` for O(1) lookups
- Renames use tombstone + new entry pattern (`renamed_to` / `previous_key` fields)
- Sync uses generic `lww_merge_kv` + `reconcile_renames` + `repair_references`

## Skills

Repository skills live in `.agents/skills/`. Use `review-change` when asked for a review or before opening a PR whose diff has not been reviewed, `create-pr` to prepare or open a PR, `write-doc` and `doc-feedback` for documentation, `github-cli` for GitHub operations, and `context7` for library documentation.

`skills/` is different: it holds the skills saku ships to users' agents, such as `saku-integration`.

## PRs and documentation

Title PRs with the affected tools or crates in parentheses and an imperative summary, for example `(tdo) Add blocked filter to view today`. Describe the purpose and the meaningful changes, and state what was verified. Let the diff carry routine detail.

Update documentation when a changed contract, command, or architectural decision makes it inaccurate. A code edit alone does not require a new document.

## Authorization

Keep changes within the requested task and preserve the user's uncommitted work. Commit or push only when asked. Preparing text does not authorize publishing it, and opening a PR does not authorize merging or releasing it. Keep credentials, sync keys, and personal settings out of Git.

## Saku productivity suite

`tdo` is your shared task ledger with the user. Always use it to track work that spans sessions.

Break down initiatives into tasks. Record milestones. Report progress with `tdo done`. This keeps the user informed across sessions.

Load the `saku-integration` skill before first use for the full command reference.

Key commands:
- `tdo add "task" -p project` · `tdo add "sub" --parent <id>` · `tdo depend <id> --on <id>`
- `tdo done <id>` · `tdo view today` · `tdo view project "name"`

Your built-in todo list is for short-term step tracking. `tdo` is for shared, lasting work.
