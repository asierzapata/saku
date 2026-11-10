# Saku (作) — Focused Terminal Tools

Lightweight, local-first tools for your work, equally accessible to humans, scripts, and agents. The current focus is `tdo`; future tools remain planned until their own development phases.

## The Suite

| Tool | Purpose | Status |
|---|---|---|
| `tdo` | Tasks, projects, scheduling, and deadlines. | Available v0.13.0; active development |
| `mdr` | Render Markdown documents inside the terminal. | Planned; not implemented |
| `sch` | Run explicitly configured commands on a local schedule. | Planned; not implemented |

See [the philosophy](documentation/PHILOSOPHY.md) for the product boundaries and [the architecture](documentation/architecture.md) for the current implementation.

## Installation

Install the task CLI from crates.io:

```bash
cargo install saku-tdo
```

Or from this checkout:

```bash
cargo install --path crates/tdo
```

To omit server sync and keychain support:

```bash
cargo install --path crates/tdo --no-default-features
```

## Usage

```bash
tdo add "Review deployment" --today
tdo view today
tdo context --json
```

In the unreleased checkout (not published v0.13 packages), SQLite cutover commands are:

```bash
tdo migrate
tdo export backup.json
tdo sync run
tdo history list --json
tdo history list --key task/...
tdo history show 1
tdo history export recovery.json
```

Migration retains the source v9 JSON ledger; sync is manual.

Recovery history stores full values from divergent pulls and merge repairs locally in plaintext, like the task database. Its source records where a value was first saved, not whether it won or lost. History is not synced, automatically pruned, or automatically restored. It cannot recover historical unseen server overwrites or classify concurrent edits without a common base. Undecryptable entries fail closed.

Guarded KV v2 atomically checks each pushed key against the completed pull cookie; a 409 triggers a pull and merge before a bounded retry. Old v1 writes receive 428, and new clients refuse old servers without fallback. Deploy the new server with all KV writers moved to v2 before live rollout. Server restart or restore invalidates the cursor and triggers a full pull/merge before local entries are republished. Recovery preserves local entries absent remotely and retains divergent versions in history. This change leaves SQLite schema v4 and v9 task JSON unchanged.

`tdo history export` writes a version 1 archive that `tdo import` cannot import. The v9 `tdo export` excludes history: export both to preserve portable content, though neither is a full operational backup.

The current CLI renders today's tasks when called without a subcommand. An interactive TUI for bare invocation is a future design goal, not part of this cleanup.

See the [command reference](documentation/tdo/commands-cheat-sheet.md) for more operations.

## Development

```bash
cargo build -p saku-tdo
cargo run -p saku-tdo -- view today
cargo test -p saku-tdo --no-default-features
```

Build the task CLI and its infrastructure together with `cargo build --release --workspace`. Binaries are written to `target/release/`.

See [build prerequisites](documentation/architecture.md#development).

## Agent Access

Agents use the same task CLI and data as humans. Explicit commands and structured output remain first-class; no agent-specific execution service is required.

The existing [integration skill and setup guide](skills/README.md) provide the task command reference and shared-ledger conventions.

## Sync

`tdo` supports optional encrypted sync through a self-hosted server. Its storage, encryption, sync, and server crates remain in the workspace. The [sync setup guide](documentation/sync-setup.md) covers current manual KV sync, coordinated rollout, backup, and restore.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).
