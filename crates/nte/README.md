# note — Agent-Friendly Note Taking CLI

A terminal-based note-taking application optimized for both human users and coding agents.

## Philosophy

- **Humans get `note edit`** — opens your preferred editor
- **Agents get verbs** — `append`, `replace`, `set` for structured mutations
- **Same files, same format** — full interoperability

---

## Commands

### Core Operations

#### `note new [title]`

Create a new note.

```bash
note new "API design decisions"
note new                           # Interactive title prompt
note new --template decision       # Use a template
note new --project tdo             # Assign to project
note new --tag architecture        # Add initial tags
```

**Options:**
| Flag | Description |
|------|-------------|
| `--template <name>` | Use a predefined template (decision, adr, session, etc.) |
| `--project <name>` | Assign to a project |
| `--tag <tag>` | Add tags (repeatable) |
| `--edit` | Open in editor immediately after creation |

---

#### `note edit <id>`

Open a note for editing in your preferred editor.

```bash
note edit 01J7X                    # By ID (prefix match)
note edit --last                   # Most recently modified
note edit --pick                   # Fuzzy finder
note edit --project tdo --pick     # Fuzzy finder within project
```

**Options:**
| Flag | Description |
|------|-------------|
| `--last` | Edit the most recently modified note |
| `--pick` | Open fuzzy finder to select note |
| `--project <name>` | Filter by project (with --pick) |
| `--preview` | Spawn a live preview pane (future) |

**Behavior:**

1. **Pre-hook:** Display note context (title, tags, project, linked items)
2. **Edit:** Spawn `$EDITOR` (falls back to `vim`)
3. **Post-hook:** Validate frontmatter, update search index, trigger sync

---

#### `note show <id>`

Display a note's contents.

```bash
note show 01J7X                    # Rendered markdown
note show 01J7X --raw              # Raw file contents
note show 01J7X --json             # JSON output for agents
note show 01J7X --meta             # Frontmatter only
```

**Options:**
| Flag | Description |
|------|-------------|
| `--raw` | Output raw markdown without rendering |
| `--json` | Output as JSON (for agent consumption) |
| `--meta` | Show only frontmatter/metadata |
| `--no-pager` | Don't pipe through pager |

---

#### `note list`

List notes with filtering.

```bash
note list                          # All notes, recent first
note list --project tdo            # Filter by project
note list --tag architecture       # Filter by tag
note list --recent 7d              # Last 7 days
note list --json                   # JSON output for agents
```

**Options:**
| Flag | Description |
|------|-------------|
| `--project <name>` | Filter by project |
| `--tag <tag>` | Filter by tag (repeatable, AND logic) |
| `--recent <duration>` | Filter by age (e.g., 7d, 2w, 1m) |
| `--limit <n>` | Maximum results |
| `--json` | JSON output |
| `--ids-only` | Output only IDs (for scripting) |

---

#### `note delete <id>`

Delete a note.

```bash
note delete 01J7X                  # Interactive confirmation
note delete 01J7X --force          # Skip confirmation
```

---

### Agent Operations

These commands provide structured, atomic mutations suitable for programmatic use.

#### `note append <id> <content>`

Append content to a note.

```bash
note append 01J7X "New insight about error handling"
note append 01J7X --section "## Open Questions" "What about retry logic?"
echo "Content from pipe" | note append 01J7X --stdin
```

**Options:**
| Flag | Description |
|------|-------------|
| `--section <heading>` | Append under a specific heading |
| `--stdin` | Read content from stdin |
| `--if-hash <hash>` | Only write if note matches hash (optimistic locking) |

---

#### `note prepend <id> <content>`

Prepend content to a note (after frontmatter).

```bash
note prepend 01J7X "TL;DR: We chose LWW for simplicity"
note prepend 01J7X --section "## Summary" "Overview text"
```

**Options:** Same as `append`

---

#### `note replace <id> --section <heading> <content>`

Replace an entire section's content.

```bash
note replace 01J7X --section "## Status" "Resolved — see PR #42"
note replace 01J7X --section "## Decision" --stdin < decision.md
```

**Options:**
| Flag | Description |
|------|-------------|
| `--section <heading>` | Target section (required) |
| `--stdin` | Read content from stdin |
| `--if-hash <hash>` | Optimistic locking |

---

#### `note set <id> [--key value...]`

Modify frontmatter metadata.

```bash
note set 01J7X --tag architecture --tag resolved
note set 01J7X --project tdo
note set 01J7X --status resolved
note set 01J7X --custom-field "any value"
```

---

#### `note unset <id> [--key...]`

Remove frontmatter metadata.

```bash
note unset 01J7X --tag wip
note unset 01J7X --project
```

---

### Query & Search

#### `note query <terms...>`

Search notes by content or metadata.

```bash
note query "sync strategy"                    # Full-text search
note query --project tdo --tag architecture   # Metadata filters
note query --semantic "how does sync work"    # Embedding search (future)
note query --json                             # JSON output
```

**Options:**
| Flag | Description |
|------|-------------|
| `--project <name>` | Filter by project |
| `--tag <tag>` | Filter by tag |
| `--recent <duration>` | Filter by age |
| `--semantic` | Use semantic/embedding search |
| `--limit <n>` | Maximum results |
| `--json` | JSON output |

---

#### `note context <file-path>`

Find notes relevant to a code file.

```bash
note context src/sync.rs           # Notes linked to this file
note context src/sync.rs --depth 2 # Include related notes
```

This is the primary agent integration point — "what do I need to know before editing this file?"

---

### Utilities

#### `note cat <id>`

Output raw note contents to stdout (for piping).

```bash
note cat 01J7X                     # Raw content
note cat 01J7X | wc -l             # Line count
```

---

#### `note path <id>`

Output the file path of a note.

```bash
note path 01J7X
# ~/.grove/notes/by-id/01J7X.md

$EDITOR $(note path 01J7X)         # Manual editing
```

---

#### `note hash <id>`

Output the content hash (for optimistic locking).

```bash
note hash 01J7X
# sha256:a1b2c3d4...
```

---

#### `note history <id>`

Show edit history of a note.

```bash
note history 01J7X
note history 01J7X --diff          # Show diffs between versions
note revert 01J7X --to <version>   # Restore previous version
```

---

#### `note link <id> [--relation <target>]`

Create relationships between notes and other entities.

```bash
note link 01J7X --blocks task:abc123        # This note blocks a task
note link 01J7X --decides 01J7Y             # This note supersedes another
note link 01J7X --context src/sync.rs       # Relevant to this code file
```

---

### Templates

#### `note template list`

List available templates.

#### `note template show <name>`

Display a template's contents.

#### `note template create <name>`

Create a new template interactively.

---

## Note Format

Notes are markdown files with YAML frontmatter:

```markdown
---
id: 01J7XKQWERTY123
title: API Design Decisions
project: tdo
tags:
  - architecture
  - sync
created: 2026-02-16T10:30:00Z
modified: 2026-02-16T14:22:00Z
links:
  - type: context
    target: src/sync.rs
  - type: blocks
    target: task:abc123
---

# API Design Decisions

Content goes here...

## Options

- Option A
- Option B

## Decision

TBD
```

---

## Configuration

Configuration lives in `~/.config/note/config.toml`:

```toml
[editor]
command = "nvim"           # Override $EDITOR
args = ["+normal G"]       # Extra arguments

[storage]
path = "~/.grove/notes"    # Note storage location

[display]
pager = "less -R"          # Pager for long output
color = true               # Colored output

[index]
auto_reindex = true        # Reindex after edits
```

---

## Environment Variables

| Variable   | Description                     |
| ---------- | ------------------------------- |
| `EDITOR`   | Preferred editor (default: vim) |
| `NOTE_DIR` | Override note storage path      |
| `NO_COLOR` | Disable colored output          |

---

## Exit Codes

| Code | Meaning                                 |
| ---- | --------------------------------------- |
| 0    | Success                                 |
| 1    | General error                           |
| 2    | Note not found                          |
| 3    | Validation error                        |
| 4    | Hash mismatch (optimistic lock failure) |
| 5    | User cancelled                          |
