# Saku (作) - Terminal Productivity Suite

A collection of focused, fast terminal tools for productivity.

## Tools

- **tdo** - Task management
- **nte** - Note taking _(planned)_
- **cal** - Calendar and events management _(planned)_
- **hbt** - Habit tracking _(planned)_
- **tmr** - Time tracking and pomodoro timers _(planned)_
- **bkm** - Bookmark management _(planned)_

## Installation

### Install tdo

```bash
cargo install --path crates/tdo
```

Or build all tools:

```bash
cargo build --release --workspace
```

The binaries will be available in `target/release/`.

## Development

This is a Cargo workspace. To build all tools:

```bash
cargo build --release --workspace
```

To work on a specific tool:

```bash
cd crates/tdo
cargo run -- today
```

## Architecture

See [documentation/architecture.md](documentation/architecture.md) for design patterns and shared utilities across the suite.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
