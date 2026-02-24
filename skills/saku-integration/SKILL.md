---
name: saku-integration
description: Guide for AI agents to use tdo, Saku's GTD task manager. Use when helping manage tasks, projects, areas, and workflows.
metadata:
  internal: false
---

# tdo Quick Reference

**Model**: Area → Project → Task | **Storage**: `~/.local/share/tdo/store.json`

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
```

> Old `tdo today`, `tdo inbox` etc. still work but are deprecated — use `tdo view <sub>`.

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
tdo done <id> [<id>...]      # complete; add --stop to cancel a recurring task
tdo delete <id>              # soft-delete (move to trash)
tdo restore <id>             # restore from trash
```

## Dependencies & Subtasks

```bash
tdo depend <blocked-id> --on <blocker-id>     # add dependency
tdo depend <blocked-id> --remove <blocker-id> # remove dependency
tdo add "Sub" --parent <parent-id>            # create subtask
```

## Areas & Projects

```bash
tdo area new "Name"                     # create area
tdo area delete "Name"                  # delete area (and all contents)
tdo project new "Name" [--area AREA]    # create project
tdo project done <slug>                 # complete project
tdo project delete <slug>              # delete project
tdo list areas|projects|tags
tdo edit area|project <name> --new-name "New"
```

> Slugs are auto-generated: "My Project" → `my-project`

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
