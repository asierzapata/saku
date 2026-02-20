# Recurring Tasks Implementation Plan

## Core Idea

A recurring task is stored **once** in the store. Occurrences are computed on the fly when rendering any view, exactly like iCalendar clients do with RRULE. Completing an occurrence appends its date to `completed_occurrences` on the task — the task itself is never marked `completed_at` until the user explicitly cancels/stops the recurrence.

---

## Supported Recurrence Patterns

| User input              | RRULE equivalent                          |
|-------------------------|-------------------------------------------|
| `daily`                 | `FREQ=DAILY`                              |
| `weekly`                | `FREQ=WEEKLY`                             |
| `monday`                | `FREQ=WEEKLY;BYDAY=MO`                    |
| `mon,wed,fri`           | `FREQ=WEEKLY;BYDAY=MO,WE,FR`             |
| `monthly`               | `FREQ=MONTHLY;BYMONTHDAY=<dtstart.day>`  |
| `1st of month`          | `FREQ=MONTHLY;BYMONTHDAY=1`              |
| `15th of month`         | `FREQ=MONTHLY;BYMONTHDAY=15`             |
| `1st monday of month`   | `FREQ=MONTHLY;BYDAY=1MO`                 |
| `last friday of month`  | `FREQ=MONTHLY;BYDAY=-1FR`                |
| `yearly`                | `FREQ=YEARLY`                             |

---

## Step 1: Data Model (`models/task.rs`)

### New types

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// A BYDAY entry: optional ordinal (1, 2, -1 for last) + weekday.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ByDay {
    pub ordinal: Option<i8>,     // None = every occurrence, 1 = first, -1 = last
    pub weekday: SerdeWeekday,
}

/// RRULE-style recurrence rule.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recurrence {
    pub freq: Freq,
    pub interval: u32,           // default 1
    pub byday: Vec<ByDay>,       // weekday constraints
    pub bymonthday: Option<i8>,  // day of month (negative = from end)
    pub until: Option<Date>,     // end date (inclusive)
    pub count: Option<u32>,      // max number of occurrences
    pub dtstart: Date,           // anchor date for generating occurrences
}

/// Serializable weekday (jiff::civil::Weekday is not Serialize).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SerdeWeekday {
    Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday,
}

impl From<SerdeWeekday> for jiff::civil::Weekday { ... }
impl From<jiff::civil::Weekday> for SerdeWeekday { ... }
```

### New `Task` fields

```rust
pub struct Task {
    // ... existing fields unchanged ...

    /// RRULE-style recurrence rule. None = one-off task.
    pub recurrence: Option<Recurrence>,

    /// Dates of occurrences that have been completed.
    pub completed_occurrences: Vec<Date>,
}
```

`Task::default()` covers both with `None` / empty `vec![]`.

### Occurrence generator

A pure function that expands a rule into dates:

```rust
/// Yields occurrence dates from `rule.dtstart` up to (and including) `up_to`.
pub fn occurrences_up_to(rule: &Recurrence, up_to: Date) -> Vec<Date>
```

Logic:
- Start from `rule.dtstart`, walk forward by `rule.interval` periods according to `rule.freq`
- Apply `byday` / `bymonthday` constraints to filter/select dates within each period
- Stop when date > `up_to`, or > `rule.until` (if set), or `rule.count` is reached

### Helper: is an occurrence pending on a given date?

```rust
pub fn is_pending_on(task: &Task, date: Date) -> bool {
    let Some(rule) = &task.recurrence else { return false };
    let occurrences = occurrences_up_to(rule, date);
    occurrences.contains(&date) && !task.completed_occurrences.contains(&date)
}
```

---

## Step 2: Recurrence Pattern Parser (`src/recurrence_parser.rs`)

New file — parses the `--every` CLI string into a `Recurrence`.

```rust
pub enum RecurrenceParseError {
    UnknownPattern(String),
}

/// Parse a user-supplied `--every` string into a `Recurrence`.
/// `dtstart` is passed in from the task's scheduled date (or today).
pub fn parse_recurrence(input: &str, dtstart: Date) -> Result<Recurrence, RecurrenceParseError>
```

Rules (case-insensitive):

| Input | Resulting `Recurrence` |
|-------|------------------------|
| `"daily"` | `freq=Daily, interval=1, byday=[], bymonthday=None` |
| `"weekly"` | `freq=Weekly, interval=1, byday=[]` |
| `"monday"` | `freq=Weekly, byday=[{None, Monday}]` |
| `"mon,wed,fri"` | `freq=Weekly, byday=[{None,Mon},{None,Wed},{None,Fri}]` |
| `"monthly"` | `freq=Monthly, bymonthday=Some(dtstart.day())` |
| `"1st of month"` | `freq=Monthly, bymonthday=Some(1)` |
| `"1st monday of month"` | `freq=Monthly, byday=[{Some(1), Monday}]` |
| `"last friday of month"` | `freq=Monthly, byday=[{Some(-1), Friday}]` |
| `"yearly"` | `freq=Yearly, interval=1` |

`until` and `count` are always `None` here — they come from `--until` / `--count` flags separately and are set by the caller after parsing.

Also add `Display` for `Recurrence` (used in UI badge):

```
Daily            → "every day"
Weekly           → "every week"
Weekdays MO,WE,FR → "Mon, Wed, Fri"
Monthly day 1    → "1st of month"
Monthly 1MO      → "1st Mon of month"
Monthly -1FR     → "last Fri of month"
Yearly           → "every year"
```

---

## Step 3: Storage Migration (`storage/migrations.rs`)

Add `migrate_v6_to_v7`:

```rust
fn migrate_v6_to_v7(value: &mut serde_json::Value) {
    if let Some(tasks) = value["tasks"].as_array_mut() {
        for task in tasks.iter_mut() {
            task["recurrence"] = serde_json::Value::Null;
            task["completed_occurrences"] = serde_json::json!([]);
        }
    }
    value["version"] = serde_json::json!(7);
}
```

Bump `CURRENT_VERSION` from `6` to `7`.

---

## Step 4: Store (`models/store.rs`)

Bump version constant to `7`.

Add query helper:

```rust
impl Store {
    /// Active tasks that have a recurrence rule set (not cancelled/deleted).
    pub fn get_recurring_tasks(&self) -> impl Iterator<Item = &Task> {
        self.get_active_tasks()
            .filter(|t| t.recurrence.is_some() && t.completed_at.is_none())
    }
}
```

---

## Step 5: Service layer (`services/tasks.rs`)

### 5a. `AddTaskParameters`

```rust
pub struct AddTaskParameters {
    // ... existing ...
    pub recurrence: Option<Recurrence>,
}
```

In `add_task`, copy `parameters.recurrence` onto the new task.

### 5b. `MoveTaskParameters`

```rust
pub struct MoveTaskParameters {
    // ... existing ...
    pub recurrence: Option<Recurrence>,
    pub clear_recurrence: bool,
}
```

### 5c. `complete_task` — two behaviors

Distinguish by whether the task is recurring:

**Non-recurring task** (existing behavior):
- Set `completed_at = now`

**Recurring task**:
- Determine the occurrence date being completed (the task's `when` date, or today)
- Append that date to `task.completed_occurrences`
- Do **not** touch `completed_at` — the task stays alive

**New service function: `cancel_recurrence`**:
- Sets `completed_at = now` on a recurring task, effectively stopping it
- Exposed as `tdo cancel <task_number>` or `tdo done --stop <task_number>`

Update `CompleteTaskResult`:

```rust
pub struct CompleteTaskResult {
    pub task: Task,
    pub newly_unblocked: Vec<Task>,
    // next_recurring_instance removed — no longer needed
}
```

---

## Step 6: View rendering (`main.rs` / view handlers)

Every view that lists tasks needs to expand recurring tasks into their pending occurrences. The key change is in the filter/display pipeline:

**Today view**: include recurring tasks where `is_pending_on(task, today)` is true.

**Upcoming view**: for each recurring task, call `occurrences_up_to(rule, end_of_window)`, subtract `completed_occurrences`, and inject virtual "occurrence rows" into the list (same task, different display date).

**Inbox / Someday / Logbook**: recurring tasks don't appear here — they always live in the scheduled space.

**Recurring view** (`tdo view recurring`):

```rust
ViewEntity::Recurring => {
    // Show all tasks with recurrence set (not cancelled), one row each.
    // Display the next pending occurrence date and the recurrence badge.
}
```

---

## Step 7: CLI (`main.rs`)

### New flags on `Add` and `Move`

```
--every <PATTERN>    Recurrence pattern ("daily", "monday", "mon,wed,fri", "1st of month", …)
--until <DATE>       Stop recurring after this date
--count <N>          Stop after N occurrences
--clear-recurrence   (Move only) Remove recurrence from the task
```

### New `done` flag

```
--stop               Cancel a recurring task permanently (sets completed_at)
```

### `tdo view recurring`

Show all recurring tasks with their rule badge and next occurrence date.

---

## Step 8: UI (`ui.rs`)

In `render_task_line`, if `task.recurrence.is_some()`, append a dimmed recurrence badge after the context:

```
42  ○  Team standup          Work  ↻ Mon, Wed, Fri   next: Mon Feb 23
43  ○  Pay rent           Personal  ↻ 1st of month   next: Sun Mar 1
```

---

## Step 9: Tests

### Unit tests in `models/task.rs`

- `occurrences_up_to` for each `Freq` variant
- Edge cases: month-end clamping (Jan 31 → Feb 28), leap years
- `count` termination
- `until` boundary (inclusive)
- `is_pending_on` returns false for completed occurrences

### Unit tests in `recurrence_parser.rs`

- All input strings parse to correct `Recurrence`
- Case-insensitive, whitespace-tolerant
- Unknown strings return error

### Integration tests (`tests/recurring_tasks.rs`)

- `tdo add "standup" --every monday` → task created
- `tdo view recurring` → task appears
- `tdo done <id>` → occurrence appended to `completed_occurrences`, task still active
- `tdo done <id> --stop` → `completed_at` set, task disappears from views
- `tdo done <id>` past `--until` → error or no-op
- Migration: v6 JSON loads cleanly as v7

---

## Files Changed Summary

| File | Change |
|------|--------|
| `crates/tdo/src/models/task.rs` | Add `SerdeWeekday`, `Freq`, `ByDay`, `Recurrence` types; add `recurrence` + `completed_occurrences` to `Task`; add `occurrences_up_to()` + `is_pending_on()` |
| `crates/tdo/src/recurrence_parser.rs` | **New file** — parse `--every` strings into `Recurrence` |
| `crates/tdo/src/storage/migrations.rs` | Add `migrate_v6_to_v7`, bump `CURRENT_VERSION` to 7 |
| `crates/tdo/src/models/store.rs` | Bump version to 7; add `get_recurring_tasks()` |
| `crates/tdo/src/services/tasks.rs` | Update `AddTaskParameters`, `MoveTaskParameters`; split `complete_task` behavior; add `cancel_recurrence` |
| `crates/tdo/src/main.rs` | Add `--every`, `--until`, `--count`, `--clear-recurrence`, `--stop` flags; add `View::Recurring`; update view pipelines |
| `crates/tdo/src/ui.rs` | Add recurrence badge + next-occurrence date to `render_task_line` |
| `crates/tdo/src/lib.rs` | `mod recurrence_parser;` declaration |
| `tests/recurring_tasks.rs` | **New file** — integration tests |

---

## Implementation Order

1. `models/task.rs` — types + `occurrences_up_to` + `is_pending_on`
2. `recurrence_parser.rs` — parser + unit tests
3. `storage/migrations.rs` + `models/store.rs` — migration + version bump
4. `services/tasks.rs` — complete_task split + cancel_recurrence
5. `main.rs` — CLI flags + view pipeline changes
6. `ui.rs` — recurrence badge
7. Integration tests
