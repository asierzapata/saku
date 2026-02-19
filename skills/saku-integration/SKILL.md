---
name: saku-integration
description: Guide for AI agents to effectively use Saku (作), a terminal productivity suite for task management. Use this when helping users manage tasks, projects, areas, and daily workflows with tdo.
metadata:
  internal: false
---

# Saku Integration Guide for AI Agents

Saku (作) is a fast, terminal-based productivity suite built in Rust. The primary tool is **tdo**, a GTD-inspired task manager.

## Data Model

```
Area (e.g., "Work", "Personal")
  └─ Project (e.g., "website-redesign")
      └─ Task (e.g., "Review PR #123")
          ├─ Title (required)
          ├─ Schedule (inbox/today/date/someday)
          ├─ Deadline (optional)
          ├─ Tags, Notes (optional)
          └─ Status (active/completed/deleted)
```

**Storage**: `~/.local/share/tdo/store.json` (override with `TDO_DATA_DIR`)

## Command Reference

### View Commands

```bash
tdo                    # Today + overdue
tdo inbox              # Uncategorized tasks
tdo upcoming           # Future tasks
tdo someday            # Someday/maybe
tdo logbook            # Completed (last 14 days)
tdo all                # All active tasks
```

### Adding Tasks

```bash
tdo add "Task" [--today|--tomorrow|--on DATE|--someday]
tdo add "Task" -p PROJECT -a AREA -t TAG --due DATE -n "notes"

# Examples
tdo add "Review PR" --today -p rust-24m -t urgent
tdo add "Call dentist" --on 2026-03-15 -a personal
```

### Managing Tasks

```bash
tdo done <id>                      # Complete task
tdo move <id> [--today|--on DATE]  # Reschedule
tdo move <id> -p PROJECT           # Reassign project
tdo delete <id>                    # Soft delete
tdo restore <id>                   # Restore from trash
```

### Projects & Areas

```bash
tdo create project "Name" [--area AREA]
tdo create area "Name"
tdo list projects|areas
tdo show project|area <name>
tdo edit project|area <name> --new-name "New Name"
```

### Tags

```bash
tdo list tags
tdo show tag <name>
```

## Best Practices for Agents

1. **Use IDs over fuzzy matching**: `tdo done 42` not `tdo done "review"`
2. **Prefer ISO dates**: `--on 2026-03-15` over `--on "next friday"`
3. **Check exit codes**: `0` = success, `1` = runtime error, `2` = validation error
4. **Create structure first**: Check if project/area exists before assignment
5. **Use move for updates**: `tdo move <id> --today -p new-project`

## Common Workflows

### Daily Planning
```bash
tdo today              # Review today
tdo inbox              # Process new items
tdo move <id> --today  # Schedule from inbox
```

### Task Capture
User: "Remind me to review the PR tomorrow"
```bash
tdo add "Review the PR" --tomorrow
```

### Project Setup
```bash
tdo create area "Work"
tdo create project "Q1 Launch" --area Work
tdo add "Design mockups" -p "Q1 Launch" --today
```

### Weekly Review
```bash
tdo logbook            # What got done
tdo upcoming           # What's coming
tdo someday            # Review someday items
```

## Date Parsing

**Natural language**: today, tomorrow, monday-sunday, next-week, next-monday
**ISO dates** (recommended): YYYY-MM-DD format (e.g., 2026-03-15)

## Error Handling

**Task not found** (exit 1): Verify ID with `tdo all` first
**Invalid date** (exit 2): Use ISO dates or valid natural language
**Project not found** (exit 1): Create project first with `tdo create project`
**Conflicting flags** (exit 2): Choose one scheduling option

## Integration Examples

### Task Capture Bot
```typescript
async function captureTask(userMessage: string) {
  const task = extractTask(userMessage);
  const result = await exec(`tdo add "${task.title}" --${task.when}`);
  return result.exitCode === 0 
    ? `✓ Added: ${task.title}`
    : `✗ Error: ${result.stderr}`;
}
```

### Daily Digest
```typescript
async function dailyDigest() {
  const today = await exec('tdo today');
  const inbox = await exec('tdo inbox');
  return `Today:\n${today.stdout}\n\nInbox:\n${inbox.stdout}`;
}
```

### Smart Scheduling
```typescript
async function scheduleTask(id: number, when: string) {
  const result = await exec(`tdo move ${id} --${when}`);
  return result.exitCode === 0 
    ? `✓ Rescheduled to ${when}`
    : `✗ ${result.stderr}`;
}
```

## Quick Reference

```bash
# View
tdo today              # What's on my plate?
tdo inbox              # What needs organizing?

# Add
tdo add "Task" --today -p project -t tag

# Update
tdo done <id>          # Complete
tdo move <id> --tomorrow  # Reschedule

# Organize
tdo create project "Name" --area work
tdo list projects

# Review
tdo logbook            # What did I finish?
```

## Key Points

- Startup time: <10ms - ideal for real-time automation
- Storage: Human-readable JSON with file locking
- Exit codes: Check `$?` after commands
- Platform: Cross-platform (macOS, Linux, Windows)
- License: AGPL-3.0-or-later
