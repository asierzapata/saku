# Saku Architecture

This document describes the architectural patterns and shared conventions across the Saku productivity suite.

For the design intent behind the suite, see [PHILOSOPHY.md](PHILOSOPHY.md). For the build order, see [ROADMAP.md](ROADMAP.md).

---

## The Suite

Tools are organized around the recurring loops of a developer's day. Each tool serves one loop and no more.

```
Daily Loop     → tdo, jrn
Knowledge Loop → dcs
Work Loop      → ctx
Orchestrator   → saku
```

### Current Status

| Tool | Loop | Description | Status |
|---|---|---|---|
| `tdo` | Daily | Task queue. Work orders for human and agent. | **Shipping** v0.5.11 |
| `jrn` | Daily | Daily journal. Chronological log of what happened. | Planned |
| `dcs` | Knowledge | Decision log. What was decided and why. | Planned |
| `ctx` | Work | Session context. Save and restore where you left off. | Planned |
| `saku` | Orchestrator | Cross-tool context, search, and sync. | Planned |

---

## Monorepo Structure

Saku is organized as a Cargo workspace:

```
saku/
├── Cargo.toml                    # Workspace configuration
├── crates/
│   ├── tdo/                      # Daily loop: task queue
│   ├── jrn/                      # Daily loop: journal (planned)
│   ├── dcs/                      # Knowledge loop: decisions (planned)
│   ├── ctx/                      # Work loop: session context (planned)
│   ├── saku/                     # Orchestrator (planned)
│   ├── saku-storage/             # Shared storage abstraction
│   ├── saku-crypto/              # Encryption utilities
│   └── saku-sync/                # Cross-device sync
├── documentation/
│   ├── PHILOSOPHY.md             # Design intent
│   ├── ROADMAP.md                # Build order and priorities
│   ├── architecture.md           # This file
│   ├── tdo/                      # tdo-specific docs
│   ├── hbt/                      # hbt-specific docs
│   └── ...
└── skills/
    └── saku-integration/         # AI agent integration guide
```

### Why a Monorepo?

- **Shared infrastructure** — Storage traits, UI patterns, and common types are reused across tools
- **Consistent versioning** — All tools evolve together with aligned dependencies
- **Unified development** — Single checkout, build, and test process
- **Cross-tool integration** — `saku context` can read from any tool's store; `ctx` can reference active `tdo` tasks

---

## Common Patterns

### Architecture Layers

Every CLI tool follows the same layered structure:

```
1. CLI Layer     (main.rs)        — Command parsing, validation, user interaction
2. Services      (services/)      — Business logic and orchestration
3. Models        (models/)        — Domain types and data structures
4. Storage       (storage/)       — Persistence abstraction (trait-based)
5. UI            (ui.rs or ui/)   — Terminal rendering and formatting
```

This structure is enforced by convention, not by a shared crate. Each tool is independently buildable and testable.

### Storage Strategy

**Single JSON file per tool:**

```
~/.local/share/<tool>/store.json    # Primary data
~/.local/share/<tool>/backups/      # Automatic versioned backups (5 kept)
```

- **In-memory**: HashMap for O(1) lookups during operation
- **Persisted**: Vec for compact JSON serialization
- **File locking**: `fs2` crate prevents concurrent write corruption
- **Schema versioning**: Migration system handles format changes across versions

**Override storage path** via environment variable `<TOOL>_DATA_DIR` (e.g., `TDO_DATA_DIR`). Used in tests and for non-default installations.

**When to consider alternatives**: Stay with JSON until a tool's store exceeds ~10,000 records and query performance becomes measurable. The rule: optimize for simplicity first, performance when proven necessary.

### UI Rendering

- `colored` crate — consistent color palette across tools
- `term_size` — responsive layouts that adapt to terminal width
- **Visual language**: each tool has a design spec in `documentation/<tool>/design-spec.md` defining glyphs, colors, and layout rules
- **Reference implementation**: `tdo/src/ui.rs` — all tools should follow its conventions for spacing, alignment, and output density

**Output modes** every tool must support:
1. Human-readable (default) — formatted, colored, terminal-width-aware
2. JSON (`--output json` or `--json`) — structured, uncolored, machine-parseable
3. CSV where tabular data makes sense (`--output csv`)

### Exit Codes

Consistent across all tools:

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Runtime error (item not found, conflict, etc.) |
| `2` | Validation error (invalid date, conflicting flags, etc.) |

### Data Model Conventions

**IDs**: Every entity has a UUID (internal) and a user-facing auto-incrementing number. Agents use UUIDs; humans use numbers. Fuzzy name matching is supported for human convenience.

**Timestamps**: Use `jiff::Timestamp` (not chrono). `HybridTimestamp` (from `saku-storage`) for entities that participate in sync conflict resolution.

**Soft deletes**: Entities are never hard-deleted. `deleted_at: Option<Timestamp>` — deleted items go to a trash view and can be restored.

**Migrations**: Every store has a `version: u32` field. When the schema changes, a migration function transforms old data to the new shape at load time.

---

## Cross-Tool Integration

Tools are independently useful but designed to compose. The integration surface is the filesystem — tools read each other's stores directly, never via network.

**Current integrations (planned):**

- `ctx` reads active `tdo` tasks when saving session context
- `saku context` reads from all tool stores to produce the combined snapshot
- `jrn` entries can reference `tdo` task IDs and `dcs` decision IDs

**Cross-tool reference format:**

When one tool references an entity in another, use the format `<tool>:<id>`:

```
nte:a3f2c1          # a note in nte
tdo:42              # task #42 in tdo
dcs:jwt-auth        # decision with slug "jwt-auth"
```

This is a convention, not enforced by the runtime today. As tools mature, `saku` will validate these references.

---

## Adding a New Tool

1. **Create the crate:**
   ```bash
   mkdir -p crates/<tool>/src
   ```

2. **Add to workspace** in root `Cargo.toml`:
   ```toml
   [workspace]
   members = ["crates/tdo", "crates/<tool>", ...]
   ```

3. **Create `Cargo.toml`:**
   ```toml
   [package]
   name = "<tool>"
   version = "0.1.0"
   edition.workspace = true
   license.workspace = true

   [[bin]]
   name = "<tool>"
   path = "src/main.rs"

   [dependencies]
   clap.workspace = true
   serde.workspace = true
   serde_json.workspace = true
   uuid.workspace = true
   jiff.workspace = true
   colored.workspace = true
   term_size.workspace = true
   thiserror.workspace = true
   ```

4. **Follow the layered architecture** — `main.rs`, `models/`, `services/`, `storage/`, `ui/`

5. **Create documentation:**
   ```
   documentation/<tool>/
   ├── design-spec.md        # visual language, glyphs, layout mockups
   └── commands-cheat-sheet.md
   ```

6. **Build and test:**
   ```bash
   cargo build --release -p <tool>
   cargo test -p <tool>
   ```

---

## Development Workflow

### Building

```bash
cargo build --release --workspace   # all tools
cargo build --release -p tdo        # specific tool
cargo build -p tdo                  # dev build (faster)
```

### Running

```bash
cargo run -p tdo -- view today
```

### Testing

```bash
cargo test --workspace
cargo test -p tdo
cargo test -- --nocapture           # show output
```

### Code Quality

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo check --workspace
```

---

## Shared Infrastructure Crates

### `saku-storage`

Storage abstraction used by all tools. Provides:
- `HybridTimestamp` — logical clock for sync conflict resolution
- Storage trait — generic persistence layer each tool implements
- Migration utilities

### `saku-crypto`

Encryption for data at rest and in transit. Used by `saku-sync` for encrypted sync payloads.

### `saku-sync`

Cross-device sync. Implements a CRDT-based merge strategy using `HybridTimestamp`. Exposed to users via the `saku sync` orchestrator command (planned).

### `saku-shared` (future)

As the suite grows, common UI components and utilities will be extracted here. Rule of three: wait until the pattern appears in at least three tools before abstracting.

---

## Configuration System

Unified configuration in `~/.config/saku/`:

```
~/.config/saku/
├── config.toml       # Suite-wide settings (planned)
├── tdo.toml          # Tool-specific overrides (planned)
└── ...
```

Tools read their config at startup. Format is TOML. Defaults are always valid — config files are optional.

**Design constraint:** Configuration options are a last resort. Prefer opinionated defaults. Add a config key only when a reasonable person would have a different preference than the default.

---

## Design Principles

1. **One loop per tool** — when a tool starts solving problems from a different loop, it has grown too large
2. **Legible to both principals** — every output must be equally parseable by a human and a program
3. **Fast by default** — sub-10ms startup, no daemon, no background work
4. **Human-readable storage** — JSON files users can inspect, edit, and back up manually
5. **Composable** — tools work together through the filesystem, not through a shared process
6. **Progressive enhancement** — start with the simplest useful thing, add complexity only when proven necessary

---

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
