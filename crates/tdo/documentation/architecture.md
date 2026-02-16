# Saku Architecture

This document describes the architecture and design patterns used across the Saku productivity suite, with a focus on the `tdo` task manager implementation.

## Overview

Saku tools are built as a collection of focused, fast terminal applications. Each tool is designed to be:
- **Fast**: Minimal startup time and quick operations
- **Focused**: Single responsibility per tool
- **File-based**: Local storage using JSON files
- **Composable**: Tools can work together via CLI

## Project Structure

```
saku/
├── crates/
│   ├── tdo/          # Task manager
│   ├── nte/          # Note taking (planned)
│   └── ...           # Other tools
├── documentation/
├── README.md
└── Cargo.toml        # Workspace configuration
```

## tdo Architecture

The `tdo` crate follows a layered architecture pattern:

```
┌─────────────────────────────────────────┐
│              CLI Layer                   │
│         (main.rs, clap)                  │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│           Services Layer                 │
│   (tasks, projects, areas, editor)       │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│            Models Layer                  │
│    (task, project, area, store)          │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│           Storage Layer                  │
│     (JSON file storage, migrations)      │
└──────────────────────────────────────────┘
```

### Layers

#### 1. CLI Layer (`main.rs`)
- **Responsibility**: Command-line interface and user interaction
- **Technology**: Uses `clap` for argument parsing
- **Pattern**: Command pattern with subcommands
- Handles user input, invokes services, displays results
- Colored terminal output via `colored` crate

#### 2. Services Layer (`services/`)
- **Responsibility**: Business logic and orchestration
- **Modules**:
  - `tasks.rs`: Task CRUD operations, scheduling logic
  - `projects.rs`: Project management
  - `areas.rs`: Area management
  - `task_editor.rs`: Interactive task editing in $EDITOR
- **Pattern**: Service layer pattern
- Each service function:
  - Takes parameters struct (e.g., `AddTaskParameters`)
  - Returns `Result<T, ServiceError>`
  - Handles validation and business rules
  - Coordinates between models and storage

#### 3. Models Layer (`models/`)
- **Responsibility**: Domain models and core data structures
- **Modules**:
  - `task.rs`: Task model with scheduling logic
  - `project.rs`: Project model with slug generation
  - `area.rs`: Area model
  - `store.rs`: Aggregate root containing all entities
- **Pattern**: Domain model pattern
- Models are serializable (via `serde`)
- Contains domain logic (e.g., date parsing, task filtering)

#### 4. Storage Layer (`storage/`)
- **Responsibility**: Data persistence and retrieval
- **Modules**:
  - `json.rs`: JSON file storage implementation
  - `migrations.rs`: Data migration system
- **Pattern**: Repository pattern via `Storage` trait
- Features:
  - Atomic file operations with file locking (`fs2`)
  - Automatic backups with rotation
  - Version detection and migrations
  - Error recovery

## Key Design Patterns

### Storage Pattern

The `Storage` trait provides an abstraction for persistence:

```rust
pub trait Storage {
    fn load(&self) -> Result<Store, StorageError>;
    fn save(&self, store: &Store) -> Result<(), StorageError>;
}
```

This allows:
- Easy testing with mock implementations
- Future support for alternative storage backends
- Clear separation of concerns

### Service Parameters Pattern

Services use parameter structs for better API design:

```rust
pub struct AddTaskParameters {
    pub title: String,
    pub when: When,
    pub project_slug: Option<String>,
    // ... other fields
}

pub fn add_task(
    storage: &impl Storage,
    params: AddTaskParameters,
) -> Result<Task, AddTaskError>
```

Benefits:
- Named parameters improve readability
- Easy to extend without breaking changes
- Self-documenting API

### Error Handling

- Uses `thiserror` for ergonomic error types
- Each layer has its own error types
- Errors are contextualized and actionable
- Exit codes:
  - `0`: Success
  - `1`: Runtime error
  - `2`: Validation error

### Data Migration System

The storage layer includes a migration system:
- Version field in JSON store
- Automatic detection of data format version
- Sequential migration application
- Backup creation before migrations

## Data Model

### Store Structure

```rust
pub struct Store {
    pub version: u32,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub areas: Vec<Area>,
    pub next_task_number: u32,
}
```

### Task Scheduling

Tasks use a `When` enum for flexible scheduling:
- `Inbox`: Unscheduled
- `Anytime`: No specific date
- `Someday`: Maybe later
- `Scheduled`: Specific date with optional evening flag

### Soft Delete Pattern

Entities support soft deletion:
- `deleted_at: Option<DateTime>` field
- Deleted items moved to trash
- Can be restored or permanently removed

## File System Layout

```
~/.local/share/tdo/
├── store.json          # Main data file
└── backups/
    ├── store.json.1    # Most recent backup
    ├── store.json.2
    └── ...             # Up to 10 backups
```

## UI Patterns

### Output Formatting
- Colored output for better readability
- Section headers with counts
- Grouped views (by date, project, etc.)
- Consistent formatting across commands

### Date Display
- Overdue tasks shown in red
- Today/Tomorrow shown as relative dates
- ISO dates for other scheduled tasks
- Deadline badges for tasks with due dates

## Testing Strategy

- **Unit Tests**: Test individual models and services
- **Integration Tests**: Test CLI commands end-to-end
- **Fixtures**: Use temporary files for storage tests
- **Property Testing**: Validate date parsing edge cases

## Performance Considerations

- **Fast Startup**: Minimal dependencies, no async runtime
- **Efficient Parsing**: JSON for human readability, future binary format possible
- **File Locking**: Prevents concurrent modification
- **Lazy Loading**: Only load data when needed

## Future Considerations

### Shared Utilities (Future)

As more Saku tools are developed, common patterns will be extracted:
- Shared date/time utilities
- Common UI components
- Storage abstractions
- Configuration management

### Sync Support (Future)

Planned features for cross-device usage:
- Git-based sync
- Conflict resolution
- Offline-first design

## Contributing

When adding new features:
1. Start with the domain model
2. Add service layer logic
3. Wire up CLI commands
4. Add tests at each layer
5. Update documentation

See [CONTRIBUTING.md](../CONTRIBUTING.md) for more details.
