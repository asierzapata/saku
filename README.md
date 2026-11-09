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

The current CLI renders today's tasks when called without a subcommand. An interactive TUI for bare invocation is a future design goal, not part of this cleanup.

See the [command reference](documentation/tdo/commands-cheat-sheet.md) for more operations.

## Development

```bash
cargo build -p saku-tdo
cargo run -p saku-tdo -- view today
cargo test -p saku-tdo --no-default-features
```

Build the task CLI and its infrastructure together with `cargo build --release --workspace`. Binaries are written to `target/release/`.

## Agent Access

Agents use the same task CLI and data as humans. Explicit commands and structured output remain first-class; no agent-specific execution service is required.

The existing [integration skill and setup guide](skills/README.md) provide the task command reference and shared-ledger conventions.

## Sync

`tdo` supports optional encrypted sync through a self-hosted server. Its storage, encryption, sync, and server crates remain in the workspace. See [sync setup](documentation/sync-setup.md); that guide still includes the legacy whole-file deployment path.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).
