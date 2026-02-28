# jrn — Daily Journal for Human-Agent Teams

A chronological work log for the human-agent team. Records what happened, who did it, and why — so the next person (human or agent) can pick up cold.

`jrn` is the "record" half of the Daily Loop. `tdo` says what needs doing; `jrn` says what actually happened.

## Philosophy

- **Humans get `jrn edit`** — compose longer entries in your preferred editor
- **Agents get `jrn log`** — fast, structured entries from the command line
- **Handoff is first-class** — the end-of-session summary is the most important entry of the day
- **Time is the spine** — entries are always organized chronologically, grouped by date

---

## Commands

### Core Operations

#### `jrn log <message>`

Append a timestamped entry to today's journal. The primary command — should feel as fast as `echo`.

```bash
jrn log "Fixed auth token refresh bug"
jrn log "Reviewed PR #42, left comments"       # Quick note
jrn log "Deployed v0.8.1 to staging" -p website # Link to project
jrn log "Completed auth refactor" --task 42     # Link to tdo task
jrn log "Found edge case in sync" -t bug        # Tag the entry
```

**Options:**
| Flag | Short | Description |
|------|-------|-------------|
| `--project <name>` | `-p` | Link to a project |
| `--task <id>` | | Reference a tdo task (stored as `tdo:<id>`) |
| `--tag <tag>` | `-t` | Add tags (repeatable) |
| `--ref <tool:id>` | | Add cross-tool reference (e.g., `dcs:jwt-auth`) |
| `--stdin` | | Read body from stdin |
| `--at <time>` | | Override timestamp (e.g., `09:30`, `2h ago`) |

**Behavior:**
- Creates an entry dated today with the current time
- Prints the entry number on success (e.g., `#7`)
- Exit code 0 on success

---

#### `jrn view [target]`

View journal entries. Default target is `today`.

```bash
jrn                                # Alias for: jrn view today
jrn view today                     # Today's entries
jrn view yesterday                 # Yesterday's entries
jrn view week                      # This week (Mon–today)
jrn view 2026-02-15                # Specific date
jrn view 2026-02-10..2026-02-15   # Date range
jrn view project "website"         # Entries linked to a project
jrn view tag "bug"                 # Entries with a specific tag
```

**Options:**
| Flag | Short | Description |
|------|-------|-------------|
| `--json` | `-j` | JSON output |
| `--csv` | `-c` | CSV output |
| `--watch` | `-w` | Watch for changes and re-render (pretty only) |
| `--all` | | Include deleted entries |
| `--project <name>` | `-p` | Filter by project |
| `--tag <tag>` | `-t` | Filter by tag (repeatable, OR logic) |
| `--author <name>` | | Filter by author (`human` or agent name) |

**View targets:**
| Target | Description |
|--------|-------------|
| `today` | Today's entries (default) |
| `yesterday` | Yesterday's entries |
| `week` | This week, Monday through today |
| `<date>` | Entries for a specific ISO date |
| `<date>..<date>` | Entries in a date range (inclusive) |
| `project <name>` | Entries linked to a specific project |
| `tag <name>` | Entries with a specific tag |
| `entry <id>` | Single entry detail view |

---

#### `jrn show <id>`

Display a single entry with full detail.

```bash
jrn show 7                         # By entry number
jrn show 7 --json                  # JSON output for agents
```

**Options:**
| Flag | Short | Description |
|------|-------|-------------|
| `--json` | `-j` | JSON output |
| `--raw` | | Raw body text only (for piping) |

---

#### `jrn edit [id]`

Open an entry in your preferred editor. Without an ID, opens a new entry for today.

```bash
jrn edit                           # New entry — opens $EDITOR with today's date
jrn edit 7                         # Edit existing entry #7
jrn edit --last                    # Edit most recent entry
```

**Options:**
| Flag | Description |
|------|-------------|
| `--last` | Edit the most recently created entry |

**Behavior:**

1. Creates a temporary file with the entry body (or empty template for new)
2. Spawns `$EDITOR` (falls back to `vim`)
3. On save: updates the entry, prints entry number
4. On empty/unchanged: aborts without changes

---

#### `jrn delete <id>`

Soft-delete an entry.

```bash
jrn delete 7                       # Interactive confirmation
jrn delete 7 --force               # Skip confirmation
```

---

#### `jrn restore <id>`

Restore a soft-deleted entry.

```bash
jrn restore 7
```

---

### Handoff

The handoff is the most important pattern in `jrn`. It's how the human-agent team exchanges state across sessions — the end-of-shift summary that lets the next person start without asking "where did we leave off?"

#### `jrn handoff <message>`

Write a handoff entry. Handoff entries are visually distinct in views and surfaced by `jrn handoff --read`.

```bash
jrn handoff "Auth fix deployed to staging. Needs prod deploy + monitoring."
jrn handoff "Finished refactoring sync module. Tests pass. Blocked on PR review." -p saku
jrn handoff --stdin < summary.md   # Longer handoff from file
```

**Options:** Same as `jrn log`, plus:
| Flag | Description |
|------|-------------|
| `--read` | Display the most recent handoff entry instead of writing |

```bash
jrn handoff --read                 # Show latest handoff (any author)
jrn handoff --read --author human  # Show latest human handoff
jrn handoff --read --json          # JSON output for agent consumption
```

**Behavior:**
- Creates an entry with `kind: handoff`
- Handoff entries get a distinct glyph (`★`) in views
- `--read` finds the most recent handoff entry and displays it

---

### Agent Operations

These commands provide structured, atomic mutations suitable for programmatic use.

#### `jrn amend <id> <message>`

Replace the body of an existing entry.

```bash
jrn amend 7 "Updated: Fixed auth bug AND the related session timeout issue"
jrn amend 7 --stdin < corrected.md
```

**Options:**
| Flag | Description |
|------|-------------|
| `--stdin` | Read new body from stdin |

---

#### `jrn set <id> [--key value...]`

Modify entry metadata.

```bash
jrn set 7 --tag bug --tag fixed
jrn set 7 --project website
jrn set 7 --ref tdo:42
```

---

#### `jrn unset <id> [--key...]`

Remove entry metadata.

```bash
jrn unset 7 --tag wip
jrn unset 7 --project
```

---

### Utilities

#### `jrn list`

List entries with filtering. More compact than `view` — one line per entry, no grouping.

```bash
jrn list                           # All entries, recent first
jrn list --recent 7d               # Last 7 days
jrn list --project website         # Filter by project
jrn list --tag bug                 # Filter by tag
jrn list --json                    # JSON output
jrn list --ids-only                # Entry numbers only (for scripting)
```

**Options:**
| Flag | Description |
|------|-------------|
| `--recent <duration>` | Filter by age (e.g., `7d`, `2w`, `1m`) |
| `--project <name>` | Filter by project |
| `--tag <tag>` | Filter by tag (repeatable, OR logic) |
| `--author <name>` | Filter by author |
| `--kind <kind>` | Filter by kind (`log`, `handoff`) |
| `--limit <n>` | Maximum results |
| `--json` | JSON output |
| `--ids-only` | Output only entry numbers |

---

#### `jrn stats [target]`

Summary statistics for a time period.

```bash
jrn stats                          # Today
jrn stats week                     # This week
jrn stats 2026-02                  # February 2026
```

Output:
```text
  Week of Feb 24                              17 entries

  By project     website 8 · saku 5 · (none) 4
  By author      human 11 · agent 6
  Handoffs       3
  Most active    Wed (6 entries)
```

---

## Entry Model

### Storage

Follows the saku KV store pattern:

```
~/.local/share/jrn/store.json       # Primary data
~/.local/share/jrn/backups/          # Automatic backups (5 kept)
```

Override with `JRN_DATA_DIR` environment variable.

### On-Disk Format

```json
{
  "version": 1,
  "entries": {
    "entry/a3f2c1b9": {
      "storage_key_suffix": "a3f2c1b9",
      "entry_number": 7,
      "body": "Fixed auth token refresh bug",
      "date": "2026-02-28",
      "time": "14:22:05",
      "kind": "log",
      "author": "human",
      "project_key": "project/website",
      "tags": ["bug", "auth"],
      "refs": ["tdo:42"],
      "created_at": "2026-02-28T14:22:05Z",
      "modified_at": { "wall_ms": 1740753725000, "lamport": 1, "device_id": "abc123" },
      "deleted_at": null
    },
    "entry/k7m2d4e1": {
      "storage_key_suffix": "k7m2d4e1",
      "entry_number": 8,
      "body": "Auth fix deployed to staging. Needs prod deploy + monitoring tomorrow.",
      "date": "2026-02-28",
      "time": "17:30:00",
      "kind": "handoff",
      "author": "human",
      "project_key": "project/website",
      "tags": [],
      "refs": ["tdo:42", "tdo:43"],
      "created_at": "2026-02-28T17:30:00Z",
      "modified_at": { "wall_ms": 1740764400000, "lamport": 2, "device_id": "abc123" },
      "deleted_at": null
    }
  }
}
```

### Entity Fields

| Field | Type | Description |
|-------|------|-------------|
| `storage_key_suffix` | `String` | 8-char base-36 hash (SHA256 of device_id + creation_ms) |
| `entry_number` | `u64` | Auto-incrementing user-facing number |
| `body` | `String` | The entry content (plain text, may be multi-line) |
| `date` | `Date` | Which day this entry belongs to |
| `time` | `String` | Time of day (`HH:MM:SS`) |
| `kind` | `EntryKind` | `log` or `handoff` |
| `author` | `String` | `human` or agent identifier |
| `project_key` | `Option<String>` | Project storage key (e.g., `project/website`) |
| `tags` | `Vec<String>` | User-defined tags |
| `refs` | `Vec<String>` | Cross-tool references (`tdo:42`, `dcs:jwt-auth`) |
| `created_at` | `Timestamp` | When the entry was created |
| `modified_at` | `HybridTimestamp` | Logical clock for sync conflict resolution |
| `deleted_at` | `Option<Timestamp>` | Soft-delete timestamp |

### Entity Trait

```rust
impl Entity for Entry {
    fn entity_type() -> &'static str { "entry" }
    fn natural_key(&self) -> String { self.storage_key_suffix.clone() }
}
```

---

## Visual Design

### Today View

Entries are grouped by date, ordered chronologically. Time on the left, entry number, body, project context on the right.

```text
$ jrn view today

  Today (Feb 28)                                4 entries

  10:14  #1  Reviewed PR #42, left comments     Work / website
  11:30  #2  Fixed auth token refresh bug        Work / website
  14:22  #3  Deployed staging build v0.8.1
  17:30  #4  ★ Auth fix on staging. Needs        Work / website
                prod deploy tomorrow.

```

### Week View

```text
$ jrn view week

  Monday, Feb 24                                3 entries

  09:15  #1  Set up dev environment for saku     saku
  11:00  #2  Reviewed sync architecture doc       saku
  16:45  #3  ★ Sync module understood. Starting   saku
                implementation tomorrow.

  Tuesday, Feb 25                               2 entries

  10:30  #4  Implemented merkle tree hashing      saku
  17:00  #5  ★ Merkle tree done, tests pass.      saku

  ...

  Today (Feb 28)                                4 entries

  10:14  #8   Reviewed PR #42, left comments     Work / website
  11:30  #9   Fixed auth token refresh bug        Work / website
  14:22  #10  Deployed staging build v0.8.1
  17:30  #11  ★ Auth fix on staging. Needs        Work / website
                 prod deploy tomorrow.

```

### Handoff View

```text
$ jrn handoff --read

  ★  Handoff #11 · Today 17:30 · human          Work / website

  Auth fix on staging. Needs prod deploy tomorrow.
  Monitoring dashboard is set up — check for 401 spikes.

  Refs  tdo:42 · tdo:43

```

### Glyphs

| Element | Glyph | Style |
|---------|-------|-------|
| Log entry | (none) | Standard foreground |
| Handoff entry | `★` | **Yellow** / bold |
| Deleted entry | `✕` | Dimmed |

### Color Palette

| Element | Color |
|---------|-------|
| Date headers | **Bold** |
| Timestamps | Dimmed |
| Entry numbers | Dimmed |
| Entry body | Standard foreground |
| Project context | Dimmed (right-aligned) |
| Handoff glyph | Yellow, bold |
| Handoff body | Standard foreground |
| Refs | Cyan |

---

## Cross-Tool Integration

### References

Entries can reference entities in other tools using the `<tool>:<id>` format:

```bash
jrn log "Completed task" --task 42           # Shorthand → tdo:42
jrn log "Per decision" --ref dcs:jwt-auth    # Explicit cross-tool ref
```

References are stored in the `refs` field and rendered in detail views.

### Reading tdo Context

`jrn` reads from `tdo`'s store to resolve task references for display:

```text
  Refs  tdo:42 Fix auth token refresh (✓ completed)
        tdo:43 Deploy auth fix to prod (○ pending)
```

This is read-only — `jrn` never writes to `tdo`'s store.

### Author Detection

The `author` field records who created the entry:
- **Human**: default when run from an interactive terminal
- **Agent**: set automatically when `JRN_AUTHOR` environment variable is present, or detected via known agent indicators (e.g., `CLAUDE_CODE` env var)

Override with `--author <name>` flag on any write command.

---

## Configuration

Configuration in `~/.config/saku/jrn.toml` (optional — defaults are always valid):

```toml
[display]
pager = "less -R"          # Pager for long output
color = true               # Colored output

[author]
default = "human"          # Default author name
```

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `EDITOR` | Preferred editor for `jrn edit` (default: `vim`) |
| `JRN_DATA_DIR` | Override storage path |
| `JRN_AUTHOR` | Override author for all entries in this session |
| `NO_COLOR` | Disable colored output |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Runtime error (entry not found, store error) |
| `2` | Validation error (invalid date, conflicting flags) |
