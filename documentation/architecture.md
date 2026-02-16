# Saku Architecture

This document describes the architectural patterns and shared conventions across the Saku productivity suite.

## Monorepo Structure

Saku is organized as a Cargo workspace with multiple CLI tools:

```
saku/
├── Cargo.toml              # Workspace configuration
├── crates/
│   ├── tdo/                # Task management
│   ├── cal/                # Calendar (planned)
│   ├── hbt/                # Habits (planned)
│   └── ...                 # Future tools
└── documentation/          # Suite-wide documentation
```

### Why a Monorepo?

- **Shared infrastructure**: UI utilities, storage patterns, and common types can be reused
- **Consistent versioning**: All tools evolve together with aligned dependencies
- **Unified development**: Single checkout, build, and test process
- **Cross-tool integration**: Future opportunities for data sharing (e.g., link tasks to calendar events)

## Common Patterns

### Architecture Layers

Each CLI tool follows a consistent layered architecture:

1. **CLI Layer** (`main.rs`): Command parsing, validation, user interaction
2. **Services Layer** (`services/`): Business logic and orchestration
3. **Models Layer** (`models/`): Core domain types and data structures
4. **Storage Layer** (`storage/`): Persistence abstraction (trait-based)
5. **UI Layer** (`ui.rs` or `ui/`): Terminal rendering and formatting

### Storage Strategy

**Current approach** (as seen in `tdo`):

- **Single JSON file** per tool in `~/.local/share/<tool>/store.json`
- **In-memory representation**: HashMap for fast lookups
- **Persisted representation**: Vec for JSON serialization
- **File locking**: Concurrent access safety via `fs2` crate
- **Backups**: Automatic versioned backups on write
- **Schema versioning**: Migration system for data format changes

**Future considerations**:

- Shared storage utilities could be extracted to `saku-shared` crate
- Cross-tool data storage (e.g., unified configuration)
- Optional database backends for power users

### UI Rendering

**Terminal UI patterns**:

- Use `colored` crate for consistent color schemes
- `term_size` for responsive layouts
- Clear visual hierarchy with borders, spacing, and alignment
- Consistent formatting across tools (see `tdo/src/ui.rs` as reference)

**Style guidelines**:

- Minimal, scannable output
- Color coding for status (e.g., green for success, red for errors)
- Table/list formatting for data presentation

### Dependencies

**Workspace-level shared dependencies**:

- `clap` (v4): CLI argument parsing with derive macros
- `serde` + `serde_json`: Serialization/deserialization
- `uuid`: Unique identifiers for entities
- `jiff`: Date and time handling (modern alternative to chrono)
- `fs2`: File locking for safe concurrent access
- `thiserror`: Ergonomic error handling
- `dirs`: Platform-aware directory paths
- `colored`: Terminal colors
- `term_size`: Terminal dimensions

**Note on Rust Edition**: The workspace uses edition "2024" to support modern Rust features like let chains. Ensure you have a recent Rust toolchain (1.83+).

**Guidelines**:

- Keep dependencies minimal and focused
- Prefer actively maintained crates with good ergonomics
- Lock versions in workspace `Cargo.toml` for consistency

## Adding a New Tool

To add a new tool to the Saku suite:

1. **Create the crate structure**:

   ```bash
   mkdir -p crates/<tool-name>/src
   ```

2. **Add to workspace members** in root `Cargo.toml`:

   ```toml
   [workspace]
   members = [
       "crates/tdo",
       "crates/<tool-name>",
   ]
   ```

3. **Create the tool's `Cargo.toml`**:

   ```toml
   [package]
   name = "<tool-name>"
   version = "0.1.0"
   edition.workspace = true
   license.workspace = true

   [[bin]]
   name = "<tool-name>"
   path = "src/main.rs"

   [dependencies]
   clap.workspace = true
   # ... other dependencies
   ```

4. **Follow the layered architecture**:
   - Start with `main.rs` for CLI parsing
   - Create `models/` for domain types
   - Create `services/` for business logic
   - Implement storage trait for persistence
   - Add UI rendering utilities

5. **Create tool documentation**:
   - Add `documentation/<tool-name>/` directory
   - Include design specs, command references, etc.

6. **Build and test**:
   ```bash
   cargo build --release -p <tool-name>
   cargo test -p <tool-name>
   ```

## Development Workflow

### Building

```bash
# Build all tools
cargo build --release --workspace

# Build specific tool
cargo build --release -p tdo

# Development build (faster, unoptimized)
cargo build -p tdo
```

### Running

```bash
# Run from workspace root
cargo run -p tdo -- today

# Run from tool directory
cd crates/tdo
cargo run -- today

# Run installed binary
tdo today
```

### Testing

```bash
# Test all tools
cargo test --workspace

# Test specific tool
cargo test -p tdo

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format all code
cargo fmt --all

# Lint all code
cargo clippy --workspace -- -D warnings

# Check without building
cargo check --workspace
```

## Future Enhancements

### Shared Core Library (`saku-shared`)

As more tools are added, extract common utilities:

- **Storage traits**: Generic persistence layer
- **UI components**: Reusable terminal rendering
- **Error types**: Common error handling
- **Configuration**: Unified settings management

**When to create**: Wait until at least 2-3 tools have duplicated code. Follow the "rule of three" for abstraction.

### Cross-Tool Integration

Opportunities for tools to work together:

- Link `tdo` tasks to `cal` calendar events
- Connect `hbt` habits to `tdo` recurring tasks
- Reference `nte` notes from tasks
- Integrate `tmr` time tracking with tasks

### Configuration System

Unified configuration in `~/.config/saku/`:

```
~/.config/saku/
├── config.toml       # Suite-wide settings
├── tdo.toml          # Tool-specific overrides
├── cal.toml
└── ...
```

## Design Principles

1. **Focused tools**: Each tool does one thing well
2. **Fast and lightweight**: Rust performance, minimal dependencies
3. **Human-readable storage**: JSON files that users can inspect/edit
4. **Composable**: Tools can work together but remain independent
5. **Terminal-first**: Optimized for keyboard-driven workflows
6. **Progressive enhancement**: Start simple, add features incrementally

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
