# `hbt` Command Cheat Sheet

## Log

| Command                                        | Description                           |
|------------------------------------------------|---------------------------------------|
| `hbt log <habit>`                              | Mark habit as done today              |
| `hbt log <habit> --amount 3`                   | Log quantity today (quantitative)     |
| `hbt log <habit> --date yesterday`             | Log for a different day               |
| `hbt log <habit> --date 2026-02-10`            | Log for a specific date               |
| `hbt log <habit> --amount 3 --date friday`     | Log quantity on a specific day        |
| `hbt log <habit> --notes "ran in the rain"`    | Log with an optional note             |
| `hbt unlog <habit>`                            | Remove today's entry                  |
| `hbt unlog <habit> --date yesterday`           | Remove entry for a specific day       |

**Notes:**
- For quantitative habits, omitting `--amount` logs at the full goal value.
- Logging again on the same day **overwrites** the previous entry.
- Fuzzy matching applies: `exercise`, `Exer`, `ex` all resolve to the first match.
- For precision, use the numeric habit ID: `hbt log 1`.

---

## View

| Command                              | Shows                                          |
|--------------------------------------|------------------------------------------------|
| `hbt`                                | Today's checklist (default)                    |
| `hbt view today`                     | Today's checklist                              |
| `hbt view week`                      | Current ISO week grid (Mon–Sun)                |
| `hbt view month`                     | Current month grid (all habits × days)         |
| `hbt view month --month 2026-01`     | Month grid for a specific month                |
| `hbt view year`                      | Year heatmap (all habits, density per day)     |
| `hbt view year --year 2025`          | Year heatmap for a specific year               |
| `hbt view year <habit>`              | Year heatmap for a single habit                |
| `hbt view stats`                     | Streak + completion table for all habits       |
| `hbt view stats <habit>`             | Detailed stats for a single habit              |
| `hbt view trash`                     | Soft-deleted habits                            |
| `hbt view habit "name"`              | Single-habit monthly calendar + stats          |
| `hbt view category "name"`           | Habits inside a specific category              |
| `hbt list habits`                    | All habits with streaks and metadata           |
| `hbt list categories`                | All categories with habit counts               |

**Notes:**
- `hbt view habit` and `hbt view category` use case-insensitive fuzzy name matching.
- `hbt habit list` and `hbt category list` are aliases for `hbt list habits` and `hbt list categories`.

---

## Habits

| Command                                           | Description                           |
|---------------------------------------------------|---------------------------------------|
| `hbt habit new "Name"`                            | Create a binary habit                 |
| `hbt habit new "Name" --unit km`                  | Create a quantitative habit           |
| `hbt habit new "Name" --unit km --goal 5`         | Create with a daily goal              |
| `hbt habit new "Name" --category health`          | Assign to a category                  |
| `hbt habit new "Name" --notes "some context"`     | Add notes                             |
| `hbt habit edit <slug>`                           | Edit in `$EDITOR`                     |
| `hbt habit delete <slug>`                         | Soft delete (moves to trash)          |
| `hbt habit archive <slug>`                        | Archive (hides from daily view)       |
| `hbt habit restore <slug>`                        | Restore from trash                    |
| `hbt habit list`                                  | List all habits (alias: `hbt list habits`) |

**Slugs:** Auto-generated from name (lowercase, spaces→hyphens).
Example: `"Cold shower"` → `cold-shower`

---

## Categories

| Command                               | Description                                      |
|---------------------------------------|--------------------------------------------------|
| `hbt category new "Name"`             | Create a category                                |
| `hbt category delete "Name"`          | Delete a category                                |
| `hbt category list`                   | List all categories (alias: `hbt list categories`) |

---

## Flags Reference

| Flag                  | Short | Description                                    |
|-----------------------|-------|------------------------------------------------|
| `--amount <n>`        | `-a`  | Quantity to log (quantitative habits)          |
| `--date <date>`       | `-d`  | Date to log for (default: today)               |
| `--notes "text"`      | `-n`  | Attach a note to an entry or habit             |
| `--unit <unit>`       | `-u`  | Unit of measurement (e.g., `km`, `pages`)      |
| `--goal <n>`          | `-g`  | Daily target for quantitative habits           |
| `--category <name>`   | `-c`  | Assign habit to a category                     |
| `--month <YYYY-MM>`   |       | Target month for `hbt view month`              |
| `--year <YYYY>`       |       | Target year for `hbt view year`                |

### Date Formats

Both `--date` and other date inputs accept:

- **Natural language:** `today`, `yesterday`, `monday`, `last-friday`, `last-week`
- **ISO dates:** `2026-02-17`

---

## AI Agent / Scripting Reference

### Exit Codes

| Code | Meaning                                             |
|------|-----------------------------------------------------|
| `0`  | Success                                             |
| `1`  | Error (habit not found, entry conflict, etc.)       |
| `2`  | Validation error (missing `--amount`, bad date)     |

### Output Format

- **Write operations** (`log`, `unlog`, `habit new`, etc.): Print confirmation + key metadata to stdout.
- **View operations** (`view today`, `view week`, `view month`, etc.): Print formatted output to stdout.
- **Errors:** Print to stderr with actionable context. Never mix errors into stdout.

### Common Error Cases

| Situation                             | Exit | Message example                                       |
|---------------------------------------|------|-------------------------------------------------------|
| Habit not found                       | `1`  | `Error: Habit 'exercie' not found`                    |
| `--amount` missing on no-goal habit   | `2`  | `Error: No goal set for 'Exercise'`                   |
| Invalid date format                   | `2`  | `Error: Invalid date 'next-thursday'`                 |
| `--amount` passed to binary habit     | `2`  | `Error: 'Read' is a binary habit, remove --amount`    |

### Best Practices for AI Agents

1. **Use numeric IDs when possible** — `hbt log 1` is unambiguous; `hbt log exercise` relies on fuzzy matching.
2. **Prefer ISO dates** — `2026-02-17` over `friday`.
3. **Check exit codes** — Do not assume success.
4. **Use explicit flags** — Always pass `--date` when logging retroactively.
5. **One action per command** — Do not chain state changes in a single invocation.
6. **Overwrite by re-logging** — To correct an entry, call `hbt log` again with the new value.
