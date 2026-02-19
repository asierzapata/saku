# Saku (作) - Terminal Productivity Suite

A collection of focused, fast terminal tools for productivity, designed to be easy to use for both humans and AI agents.

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

## AI Agent Integration

Saku is designed to be easy for AI agents to use. To help AI agents integrate with Saku effectively, you can install the agent skill:

```bash
# Using npx (requires Vercel AI SDK skills CLI)
npx skills add https://github.com/asierzapata/saku.git

# Or manually copy the skill
cp -r skills/saku-integration .claude/skills/
# or
cp -r skills/saku-integration .cursor/skills/
```

The skill provides comprehensive guidance for AI agents on:
- Command patterns and best practices
- Common workflows (daily planning, project management, task capture)
- Error handling and exit codes
- Integration examples

See [skills/saku-integration/SKILL.md](skills/saku-integration/SKILL.md) for details.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
