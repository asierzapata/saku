# HBT CLI Design Specification

`hbt` is a terminal habit tracker with the visual language of a GitHub contribution graph. The philosophy is **one glyph per day**—visual density equals consistency. Output is minimal and human-readable, designed to work equally well for direct use and scripting.

---

## Core Concepts

| Concept      | Description |
|--------------|-------------|
| **Habit**    | A recurring behavior tracked on a daily basis |
| **Entry**    | A single day's record for a habit (binary: done/not-done, or quantitative: an amount) |
| **Streak**   | Consecutive days on which a habit was completed |
| **Category** | Optional grouping of habits (mirrors `area` in `tdo`) |

---

## Habit Types

### Binary
The simplest type: done or not done. No amount required.

```bash
hbt habit new "Cold shower"
hbt log cold-shower          # marks today as done
```

### Quantitative
Tracks a measurable amount per day. Requires a `--unit`. An optional `--goal` sets the daily target; reaching or exceeding it counts as completion.

```bash
hbt habit new "Exercise" --unit km --goal 5
hbt log exercise             # logs at full goal (5km) → done
hbt log exercise --amount 3  # logs 3km → partial (below goal)
```

If no `--amount` is given for a quantitative habit, it defaults to the `--goal` value. If no goal is set, `--amount` is required.

---

## Data Models

```rust
pub struct Habit {
    pub id: Uuid,
    pub habit_number: u64,          // User-facing auto-increment ID
    pub name: String,
    pub slug: String,               // Auto-generated from name
    pub notes: Option<String>,
    pub category_id: Option<Uuid>,
    pub unit: Option<String>,       // None = binary; Some("km") = quantitative
    pub goal: Option<f64>,          // Daily target (quantitative only)
    pub archived_at: Option<Timestamp>,
    pub deleted_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub struct Entry {
    pub id: Uuid,
    pub habit_id: Uuid,
    pub date: Date,                 // The day this entry covers
    pub amount: Option<f64>,        // None = binary check; Some(x) = amount logged
    pub notes: Option<String>,
    pub created_at: Timestamp,
}

pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub deleted_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub struct Store {
    pub version: u32,
    pub next_habit_number: u64,
    pub habits: HashMap<Uuid, Habit>,
    pub entries: HashMap<Uuid, Entry>,
    pub categories: HashMap<Uuid, Category>,
}
```

**Completion logic:**
- Binary habit: has an entry for the day → **done**
- Quantitative habit: entry with `amount >= goal` → **done**; entry with `amount < goal` → **partial**; no entry → **missed/pending**

---

## Storage

```
~/.local/share/hbt/store.json
~/.local/share/hbt/backups/
```

Same storage strategy as `tdo`: single JSON file, file-locked writes, automatic timestamped backups (keeps the 5 most recent), schema versioning with migrations.

---

## Visual Language

### Today view status glyphs

| Glyph | State                                  | Color   |
|-------|----------------------------------------|---------|
| `✓`   | Completed (binary or goal met)         | Green   |
| `▪`   | Partial (quantitative, below goal)     | Yellow  |
| `○`   | Pending (today, not yet logged)        | White   |

### Calendar / grid cell glyphs

| Glyph | State                                  | Color         |
|-------|----------------------------------------|---------------|
| `■`   | Done (binary or goal met)              | Green         |
| `▪`   | Partial (quantitative, below goal)     | Yellow        |
| `□`   | Missed (past day, not logged)          | Dimmed        |
| `·`   | Future                                 | Dimmed        |
| `○`   | Today, not yet logged                  | White         |

### Heatmap density glyphs (year view, all-habits summary)

Represents the percentage of habits completed on that day:

| Glyph | Completion |
|-------|------------|
| `□`   | 0%         |
| `░`   | 1–33%      |
| `▒`   | 34–66%     |
| `▓`   | 67–99%     |
| `■`   | 100%       |

---

## Views

### Today — `hbt` / `hbt today`

Default view. Today's habits as a checklist with streaks right-aligned.

```text
$ hbt

  Today (Feb 17)                                     3 / 5 done

  ✓  Exercise (5 / 5km)                                  14d streak
  ✓  Read                                                  7d streak
  ○  Meditate                                              3d streak
  ○  Cold shower                                          12d streak
  ○  Journal                                               6d streak

```

- The right column shows the current streak, **dimmed**.
- For quantitative habits, logged amount vs. goal is shown in parentheses after the name.
- Completed lines use the full glyph; pending lines use `○`.
- `▪` is used for partial quantitative completions:

```text
  ▪  Exercise (3 / 5km)                                  14d streak
```

---

### Week — `hbt week`

Current ISO week (Mon–Sun). Future days shown with `·`.

```text
$ hbt week

  Week (Feb 16–22)                                       5 habits

  Habit              Mo Tu We Th Fr Sa Su
  Exercise           ■  □  ■  ■  ○  ·  ·
  Read               ■  ■  □  ■  ✓  ·  ·
  Meditate           ■  ■  ■  □  ○  ·  ·
  Cold shower        □  ■  ■  ■  ○  ·  ·
  Journal            ■  ■  ■  ■  ✓  ·  ·

```

- Today's column uses `○` (pending) or `✓`/`▪` (logged).
- Past days use `■`, `▪`, or `□`.

---

### Month — `hbt month [--month YYYY-MM]`

All habits as rows, days of the month as columns.

```text
$ hbt month

  February 2026                                          5 habits

  Habit               1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17
  Exercise            ■  ■  ■  ■  ■  □  ■  ■  ■  ■  ■  □  ■  ■  ■  ■  ○
  Read                ■  ■  □  ■  ■  ■  ■  ■  □  ■  ■  ■  ■  ■  ■  ■  ✓
  Meditate            □  ■  ■  ■  □  □  ■  ■  ■  ■  □  □  ■  □  ■  ■  ○
  Cold shower         ■  ■  ■  □  ■  □  ■  ■  ■  □  ■  ■  □  ■  ■  ■  ○
  Journal             ■  □  ■  ■  ■  ■  ■  □  ■  ■  ■  ■  ■  ■  ■  ■  ✓

```

Designed for up to 80 terminal columns. If the terminal is wider, remaining days scroll to the right naturally.

---

### Year — `hbt year [--year YYYY] [<habit>]`

GitHub contribution graph layout: rows are days of the week (Mon–Sun), columns are weeks, month labels appear at the top.

**All habits (heatmap — density = % of habits done that day):**

```text
$ hbt year

  2026                                               All habits

       Jan   Feb   Mar   Apr   May   Jun   Jul   Aug   Sep   Oct   Nov   Dec
  Mon  ■ ■   ■ ▓   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Tue  ▒ ▓   □ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Wed  ■ □   ▒ □   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Thu  □ ▓   □ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Fri  ▒ ▒   ■ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Sat  □ □   □ □   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Sun  ■ ■   ■ ▒   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·

```

**Single habit (binary — done/missed per day):**

```text
$ hbt year exercise

  Exercise - 2026                         152 / 365  ·  41%  ·  14d streak

       Jan   Feb   Mar   Apr   May   Jun   Jul   Aug   Sep   Oct   Nov   Dec
  Mon  ■ ■   ■ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Tue  □ ■   □ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Wed  ■ □   ■ □   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Thu  □ ■   □ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Fri  ■ ■   ■ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Sat  □ □   □ □   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·
  Sun  ■ ■   ■ ■   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·   · ·

```

For quantitative habits in the single-habit year view, the density glyphs `░ ▒ ▓ ■` represent % of goal (same scale as the all-habits heatmap).

---

### Stats — `hbt stats [<habit>]`

**All habits:**

```text
$ hbt stats

  Stats                                                  5 habits

  Habit              Streak  Best     7d    30d  All-time
  Exercise              14d   30d   100%    87%       72%
  Read                   7d   21d   100%    89%       81%
  Meditate               3d   14d    57%    41%       45%
  Cold shower           12d   12d    86%    58%       51%
  Journal                6d   18d    86%    71%       68%

```

**Single habit:**

```text
$ hbt stats exercise

  Exercise                                           14d streak  ·  72%

  Current streak:     14 days
  Best streak:        30 days
  Last 7 days:       100%  (7 / 7)
  Last 30 days:       87%  (26 / 30)
  This month:         76%  (13 / 17)
  All-time:           72%  (142 / 197)

```

For quantitative habits, a total amount line is appended:

```text
  All-time:           72%  (142 / 197 days)  ·  789.5km total
```

---

### Habit view — `hbt habit view <slug>`

Single-habit calendar (current month) with a stats footer.

```text
$ hbt habit view exercise

  Exercise                                               14d  ·  72%

  February 2026
  Mo Tu We Th Fr Sa Su
   ·  ·  ■  ■  ■  □  ■
   ■  ■  ■  ■  ■  □  ■
   ■  ■  ■  ■  ■  □  ■
   ■  ■  ○

  Current streak: 14 days  ·  Best: 30 days  ·  142 / 197 days (72%)

```

---

### Habit list — `hbt habit list`

```text
$ hbt habit list

  Habits                                                 5 habits

  1  Exercise          14d streak    Health    km / 5km goal
  2  Read               7d streak    Mind
  3  Meditate           3d streak    Health
  4  Cold shower       12d streak
  5  Journal            6d streak    Mind

```

---

### Category view — `hbt category view <slug>`

```text
$ hbt category view health

  Health                                                 2 habits

  1  Exercise          14d streak    km / 5km goal
  3  Meditate           3d streak

```

---

### Trash — `hbt trash`

```text
$ hbt trash

  Trash                                                  1 habit

  6  Journaling (deleted Feb 10)

```

---

## Write Operation Output

All mutating commands print a short confirmation to stdout.

### `hbt log`

```text
✓ Logged: Exercise
  #1  today  14d streak
```

With explicit amount:

```text
✓ Logged: Exercise
  #1  3 / 5km  today  14d streak
```

### `hbt unlog`

```text
✓ Unlogged: Exercise
  #1  today
```

### `hbt habit new`

```text
✓ Habit created: Exercise
  #1  slug: exercise  category: health  unit: km  goal: 5km
```

### `hbt habit delete`

```text
✓ Habit deleted: Exercise
  #1  moved to trash
```

### `hbt habit archive`

```text
✓ Habit archived: Exercise
  #1  hidden from daily view  data preserved
```

---

## Error Output

Errors go to **stderr**. The message is always followed by actionable context.

```text
Error: Habit 'exercie' not found
  Did you mean: exercise?
  Run `hbt habit list` to see all habits
```

```text
Error: No goal set for 'Exercise'
  Provide --amount or set a goal with `hbt habit edit exercise`
```

```text
Error: Entry already exists for 'Exercise' on Feb 17
  Use `hbt log exercise --amount 6` to overwrite, or `hbt unlog exercise` to remove
```

```text
Error: Invalid date 'next-thursday'
  Supported formats: today, yesterday, monday, 2026-02-17
```

---

## Color and Typography Palette

| Element                        | Treatment                                   |
|--------------------------------|---------------------------------------------|
| View headers                   | Bold                                        |
| Habit names                    | Standard terminal foreground                |
| `✓` glyph                      | Green                                       |
| `▪` glyph (partial)            | Yellow                                      |
| `○` glyph (pending)            | Standard foreground                         |
| `■` cell (done)                | Green                                       |
| `▪` cell (partial)             | Yellow                                      |
| `□` cell (missed)              | Dimmed                                      |
| `·` cell (future)              | Dimmed                                      |
| Streak / context               | Dimmed (right-aligned, recedes visually)    |
| Error messages                 | Red (stderr)                                |

---

## Architecture

Follows the same layered architecture as `tdo`:

```
src/
  main.rs          # CLI parsing (clap derive), error formatting, output
  lib.rs           # Module re-exports
  models/
    habit.rs       # Habit, HabitType
    entry.rs       # Entry
    category.rs    # Category
    store.rs       # Store + StoredStore (HashMap ↔ Vec for JSON)
  services/
    habits.rs      # add_habit, delete_habit, archive_habit, restore_habit
    entries.rs     # log_entry, unlog_entry
    categories.rs  # add_category, delete_category
    stats.rs       # compute_streak, completion_rates
  storage/
    mod.rs         # Storage trait
    json.rs        # JsonFileStorage (file lock, backups, migrations)
  ui/
    mod.rs
    today.rs       # Today checklist renderer
    grid.rs        # Week / month grid renderer
    year.rs        # GitHub-style year heatmap renderer
    stats.rs       # Stats table renderer
```

---

## Exit Codes

| Code | Meaning                                            |
|------|----------------------------------------------------|
| `0`  | Success                                            |
| `1`  | Error (habit not found, entry conflict, etc.)      |
| `2`  | Validation error (invalid date, missing --amount)  |
