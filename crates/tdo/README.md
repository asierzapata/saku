# tdo - Task Management CLI

A flexible task management tool for the terminal with support for areas, projects, and different scheduling modes.

## Features

- **Flexible scheduling**: Today, Inbox, Upcoming, Anytime, Someday, or Logbook
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
tdo anytime         # Tasks to do anytime
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

## Data Storage

Tasks are stored in `~/.local/share/tdo/store.json`. The file is human-readable JSON with automatic backups.

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
