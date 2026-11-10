---
name: saku-integration
description: Guide for AI agents to use tdo, Saku's GTD task manager. Use when helping manage tasks, projects, areas, and workflows.
metadata:
  internal: false
---

# tdo Quick Reference (unreleased checkout)

**Model**: Area → Project → Task | **Storage**: `~/.local/share/tdo/store.sqlite3`; portable task exports use v9 JSON.

Bare tdo opens the local TUI only when stdin and stdout are interactive terminals; otherwise it prints usage without opening storage. Agents should use explicit subcommands such as tdo view today --json. The TUI does not synchronize automatically.

Inspect local plaintext merge history with `tdo history list --json`, `tdo history list --key task/...`, or `tdo history show 1`. `tdo history export recovery.json` writes a separate v1 archive, not importable via `tdo import`; v9 task exports exclude history. History is not synced, automatically pruned, or automatically restored. Source records first provenance, not conflict classification; historical unseen overwrites cannot be recovered, and concurrent edits cannot be classified without a common base. Undecryptable entries fail closed. Export both archives to preserve portable content, not a full operational backup.

Guarded KV v2 uses startup-bound cookies (`<server-instance>:<sequence>`). The server generates a new random instance ID outside its database at every startup, so normal restarts and stop/restore/start recovery invalidate old cookies. HTTP 410 makes the client atomically requeue all local entries and clear its cursor, then fully pull and merge before guarded pushes. Missing remote entries do not delete local entries; tombstones and divergent history are preserved. Stale writes return 409; stale-write and cursor-recovery retries share a limit of three retries. V1 writes return 428, and new clients refuse old servers without fallback. SQLite schema v4 and v9 JSON are unchanged.

## View

```bash
tdo view today|inbox|upcoming|someday|logbook|trash|all|recurring|deadlines
tdo view area "name"      # projects + tasks in area
tdo view project "name"   # tasks in project
tdo view tag "name"       # tasks with tag
tdo view task <id>        # full detail of one task
tdo view ... --json       # machine-readable output
tdo view ... --csv        # CSV output
tdo view ... --watch      # live-reload
tdo view ... --all        # include completed tasks
```

> Old `tdo today`, `tdo inbox` etc. still work but are deprecated — use `tdo view <sub>`.
> `tdo show` is an alias for `tdo view` (same commands, kept for compatibility).

## Add / Move

```bash
tdo add "Title" [SCHEDULE] [OPTIONS]
tdo move <id> [SCHEDULE] [OPTIONS]   # update any field; can take multiple IDs
```

**SCHEDULE** (mutually exclusive): `--today` `--tomorrow` `--next-week` `--someday` `--on DATE`

**OPTIONS**: `--due DATE` `-p PROJECT` `-a AREA` `-t TAG` `-n "notes"` `--every PATTERN` `--until DATE` `--parent <id>`

`tdo move` extras: `--clear-schedule` `--clear-deadline` `--clear-recurrence`

## Complete / Delete / Restore

```bash
tdo done <id> [<id>...]           # complete task(s)
tdo done <id> --note "message"    # complete with a note
tdo done <id> --stop              # complete and stop recurring task from repeating
tdo delete <id>                   # soft-delete (move to trash)
tdo restore <id>                  # restore from trash
```

## Dependencies & Subtasks

```bash
tdo depend <blocked-id> --on <blocker-id>     # add dependency
tdo depend <blocked-id> --remove <blocker-id> # remove dependency
tdo add "Sub" --parent <parent-id>            # create subtask
```

## Areas & Projects

```bash
tdo create area "Name"                  # create area
tdo remove area "Name"                  # delete area (and all contents)
tdo create project "Name" [--area AREA] # create project
tdo done <slug>                         # complete project (via move command)
tdo remove project <slug>               # delete project
tdo list areas|projects|tags
tdo edit area|project <name> --new-name "New"
```

> Slugs are auto-generated: "My Project" → `my-project`

## Context (orientation snapshot)

```bash
tdo context              # full situational snapshot (pretty)
tdo context --json       # machine-readable snapshot
```

Shows: today summary, ready tasks, blocked tasks, overdue, recent completions, inbox count, active projects.

## Completions

```bash
tdo completion bash|zsh|fish|powershell   # generate shell completions
```

## Dates

Natural: `today` `tomorrow` `monday`–`sunday` `next-week` `next-monday`
ISO (preferred): `2026-03-15`

## Exit Codes & Gotchas

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Runtime error (task/project not found) |
| 2 | Validation error (conflicting flags, bad date) |

- **`tdo remove`** is for areas/projects only — use **`tdo delete`** for tasks
- Prefer numeric IDs over fuzzy names (`tdo done 42` not `tdo done "review"`)
- Prefer ISO dates over natural language for scripting
- Check project/area exists before assigning (`tdo list projects`)

## Agent Conventions

### Tags

| Tag | Meaning |
|-----|---------|
| `agent` | Task assigned to an AI agent |
| `needs-review` | Agent completed work, human review needed |
| `blocked-human` | Agent cannot proceed, human input required |

### Agent Queue

```bash
tdo view today --tag agent --ready    # agent's actionable tasks (excludes blocked)
tdo view inbox --tag agent --ready    # agent inbox tasks
```

### Human Review Queue

```bash
tdo view inbox --tag needs-review     # tasks awaiting human review
tdo view today --tag blocked-human    # tasks blocked on human input
```

### Workflows

**Agent starting a session:**
```bash
tdo context --json                          # full orientation snapshot
tdo view today --tag agent --ready --json   # agent-specific actionable tasks
```

**Agent completing work:**
```bash
tdo done 42 --note "Implemented feature X, added tests"
tdo move 42 -t needs-review          # flag for human review (before completing)
```

**Human reviewing agent work:**
```bash
tdo view inbox --tag needs-review
tdo done 42 --note "Reviewed and approved"
```

**Agent blocked on human input:**
```bash
tdo move 42 -t blocked-human --note "Need clarification on API design"
```
