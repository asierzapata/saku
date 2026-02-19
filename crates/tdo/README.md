# tdo - Task Management CLI

A flexible task management tool for the terminal with support for areas, projects, and different scheduling modes.

## Features

- **Flexible scheduling**: Inbox, Today (scheduled dates), Upcoming, Someday, or Logbook
- **Areas & Projects**: Organize tasks by life areas and projects
- **Tags**: Cross-cutting categorization
- **Simple storage**: JSON file-based, human-readable
- **Fast**: Rust-powered performance

## Installation

```bash
cargo install --path .
```

## Usage

### View tasks

```bash
tdo today           # Tasks scheduled for today
tdo inbox           # Unprocessed tasks
tdo upcoming        # Upcoming scheduled tasks
tdo someday         # Someday/maybe tasks
tdo logbook         # Completed tasks
tdo all             # All active tasks
```

### Manage tasks

```bash
tdo add "Task description" --area work
tdo add "Task with project" --project my-project
tdo done <task-id>
tdo move <task-id> --to today
```

### Areas & Projects

```bash
tdo area new "Work"
tdo area list
tdo project new "Launch" --area work
tdo project list
```

For detailed command documentation, see [documentation/tdo/commands-cheat-sheet.md](../../documentation/tdo/commands-cheat-sheet.md).

## Logging and Debugging

By default, `tdo` is compiled **without logging** for maximum performance and minimal binary size.

### Enabling Logging

To enable structured logging and tracing, compile with the `logging` feature:

```bash
# Development build with logging
cargo build --features logging

# Release build with logging (if needed for debugging)
cargo build --release --features logging

# Install with logging
cargo install --path . --features logging
```

### Configuring Log Output

When logging is enabled, control the verbosity with environment variables:

```bash
# Error level only (default when logging feature is enabled)
TDO_LOG=error tdo <command>

# Info level - shows operation outcomes
TDO_LOG=info tdo add "My task"

# Debug level - shows detailed traces including storage operations
TDO_LOG=debug tdo add "My task"

# Trace level - most verbose, shows everything
TDO_LOG=trace tdo <command>

# Module-specific logging (debug for tdo, error for dependencies)
TDO_LOG=saku_tdo=debug tdo <command>
```

You can also use `RUST_LOG` instead of `TDO_LOG` if preferred.

### Log Output Locations

When logging is enabled, logs are written to:
- **stderr** - Real-time colored output for development
- **Log file** - `~/.local/share/tdo/tdo.log.YYYY-MM-DD` (rotates daily)

The log files persist across runs and include:
- Timestamp and log level for each entry
- Span context showing operation hierarchy
- Structured fields (task numbers, file paths, etc.)
- Full file paths and line numbers for debugging

### Example Logging Output

```bash
$ TDO_LOG=info tdo add "Test task"
2026-02-18T22:04:51.106Z  INFO saku_tdo::logging: tdo logging initialized version="0.3.1"
2026-02-18T22:04:51.108Z  INFO tdo_app:load: Store loaded successfully tasks=26 projects=11 areas=2
2026-02-18T22:04:51.109Z  INFO tdo_app:add_task: Adding new task title="Test task" when=Inbox
2026-02-18T22:04:51.110Z  INFO tdo_app:add_task:save: Store saved successfully
2026-02-18T22:04:51.110Z  INFO tdo_app:add_task: Task added successfully task_number=27
✓ Task added: Test task
  #27
```

### Why is Logging Disabled by Default?

Structured logging adds dependencies and increases binary size. For a CLI tool focused on speed and simplicity, this overhead is unnecessary for most users. The logging feature is available when you need it for debugging or monitoring, but doesn't impact the default user experience.

## Data Storage

Tasks are stored in `~/.local/share/tdo/store.json`. The file is human-readable JSON with automatic backups.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
