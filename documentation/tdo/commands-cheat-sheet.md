# `tdo` Command Cheat Sheet

## Capture

| Command                                | Description                |
| -------------------------------------- | -------------------------- |
| `tdo add "task"`                       | Add to Inbox               |
| `tdo add "task" --today`               | Add to Today               |
| `tdo add "task" --today --evening`     | Add to Today (evening tag) |
| `tdo add "task" --someday`             | Add to Someday             |
| `tdo add "task" --anytime`             | Add to Anytime             |
| `tdo add "task" --when friday`         | Schedule for specific date |
| `tdo add "task" --deadline 2025-03-01` | Set hard deadline          |
| `tdo add "task" -p project-slug`       | Add to project             |
| `tdo add "task" -a area-name`          | Add to area                |
| `tdo add "task" -t tag1 -t tag2`       | Add with tags              |
| `tdo add "task" -n "some notes"`       | Add with notes             |

**Note:** Only one scheduling flag allowed: `--today`, `--someday`, `--anytime`, or `--when` (mutually exclusive)

## View

| Command              | Shows                         |
| -------------------- | ----------------------------- |
| `tdo`                | Today (default)               |
| `tdo today`          | Today + overdue               |
| `tdo inbox`          | Uncategorized tasks           |
| `tdo upcoming`       | Future-dated, grouped by date |
| `tdo anytime`        | No date, not someday          |
| `tdo someday`        | Explicitly deferred           |
| `tdo logbook`        | Completed (last 14 days)      |
| `tdo trash`          | Soft-deleted                  |
| `tdo all`            | Everything active             |
| `tdo project list`   | List all projects             |
| `tdo project <slug>` | Tasks in specific project     |
| `tdo area list`      | List all areas                |
| `tdo area <slug>`    | Projects in specific area     |
| `tdo tag list`       | List all tags                 |
| `tdo tag <slug>`     | Tasks with specific tag       |

**Notes:**

- These are read-only view commands. To modify task scheduling, use `tdo move <id>` (see Move / Schedule section)
- Fuzzy matching applies to `done` command with title matching (case-insensitive substring search)

## Act on Tasks

| Command                  | Description                         |
| ------------------------ | ----------------------------------- |
| `tdo done <id>`          | Complete task by ID                 |
| `tdo done "fuzzy match"` | Complete by title match (first hit) |
| `tdo edit <id>`          | Edit in `$EDITOR`                   |
| `tdo delete <id>`        | Move to trash                       |
| `tdo restore <id>`       | Restore from trash                  |

**Note:** Fuzzy matching uses case-insensitive substring search. If multiple matches exist, the first active task is selected.

## Move / Schedule

The `move` command updates task properties. It supports all the same flags as `add` (see Flags Reference).

| Command                               | Description                     |
| ------------------------------------- | ------------------------------- |
| `tdo move <id> --today`               | Move task to Today              |
| `tdo move <id> --today --evening`     | Move task to Today (evening)    |
| `tdo move <id> --someday`             | Move task to Someday            |
| `tdo move <id> --anytime`             | Move task to Anytime            |
| `tdo move <id> --when friday`         | Schedule task for specific date |
| `tdo move <id> -p project-slug`       | Assign task to project          |
| `tdo move <id> -a area-name`          | Assign task to area             |
| `tdo move <id> -t new-tag`            | Add tag to task                 |
| `tdo move <id> -n "updated notes"`    | Update task notes               |
| `tdo move <id> --deadline 2025-03-01` | Set/update hard deadline        |

**Notes:**

- Flags can be combined. Example: `tdo move 5 --today -p work -t urgent`
- Only one scheduling flag allowed: `--today`, `--someday`, `--anytime`, or `--when` (mutually exclusive)
- To _view_ lists (Today, Someday, etc.), use commands without `<id>` (see View section)

## Projects

| Command                              | Description      |
| ------------------------------------ | ---------------- |
| `tdo project new "Name"`             | Create project   |
| `tdo project new "Name" --area work` | Create in area   |
| `tdo project done <slug>`            | Complete project |
| `tdo project delete <slug>`          | Delete project   |

**Project Slugs:** Auto-generated from name (lowercase, spaces→hyphens, special chars removed).
Example: "My Cool Project" → `my-cool-project`

## Areas

| Command                  | Description    |
| ------------------------ | -------------- |
| `tdo area new "Name"`    | Create area    |
| `tdo area delete "Name"` | Delete area    |
| `tdo area list`          | List all areas |

**Area names are freeform strings. No slugification applied.**

## Flags Reference

| Flag                | Short | Description                    |
| ------------------- | ----- | ------------------------------ |
| `--today`           |       | Schedule for today             |
| `--evening`         |       | Tag as evening (metadata only) |
| `--someday`         |       | Defer to someday               |
| `--anytime`         |       | Available anytime              |
| `--when <date>`     | `-w`  | Schedule for date              |
| `--deadline <date>` | `-d`  | Hard due date                  |
| `--project <slug>`  | `-p`  | Assign to project              |
| `--area <name>`     | `-a`  | Assign to area                 |
| `--tag <name>`      | `-t`  | Add tag (repeatable)           |
| `--notes "text"`    | `-n`  | Add notes                      |

### Date Formats

Both `--when` and `--deadline` accept:

- **Natural language:** `today`, `tomorrow`, `friday`, `next-monday`, `next-week`
- **ISO dates:** `2025-03-01`, `2025-12-25`

Examples:

```bash
tdo add "Review PR" --when tomorrow
tdo add "Tax filing" --deadline 2026-04-15
tdo move 42 --when next-friday
```

## AI Agent / Scripting Reference

### Exit Codes

- `0` - Success
- `1` - Error (task not found, invalid input, etc.)
- `2` - Validation error (conflicting flags, invalid date format)

### Output Format

- **Write operations** (add, done, edit): Print task ID on success
- **View operations** (inbox, today, etc.): Print formatted task list
- **Errors**: Print error message to stderr

### Common Error Cases

- **Task not found:** Returns exit code 1 with message "Task not found: <id>"
- **Multiple fuzzy matches:** Uses first match (consider using ID for precision)
- **Invalid date:** Returns exit code 2 with message "Invalid date format: <input>"
- **Conflicting flags:** Returns exit code 2 with message listing conflicts

### Best Practices for AI Agents

1. **Use IDs when available** - More precise than fuzzy matching
2. **Prefer ISO dates** - More unambiguous than natural language
3. **Check exit codes** - Don't assume success
4. **Use explicit flags** - `--when friday` clearer than relying on defaults
5. **One action per command** - Avoid chaining multiple state changes
6. **Use move for task updates** - `tdo move <id> --when tomorrow` is more explicit and flexible than hypothetical separate scheduling commands
