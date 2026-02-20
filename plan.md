# Recurring Tasks Implementation Plan

## Core Idea

A recurring task is stored **once** in the store. Occurrences are computed on the fly when rendering any view, exactly like iCalendar clients do with RRULE. Completing an occurrence appends its date to `completed_occurrences` on the task — the task itself is never marked `completed_at` until the user explicitly stops the recurrence.

---

## Supported Recurrence Patterns

| User input              | RRULE equivalent                         |
|-------------------------|------------------------------------------|
| `daily`                 | `FREQ=DAILY`                             |
| `weekly`                | `FREQ=WEEKLY`                            |
| `monday`                | `FREQ=WEEKLY;BYDAY=MO`                   |
| `mon,wed,fri`           | `FREQ=WEEKLY;BYDAY=MO,WE,FR`            |
| `monthly`               | `FREQ=MONTHLY;BYMONTHDAY=<dtstart.day>` |
| `1st of month`          | `FREQ=MONTHLY;BYMONTHDAY=1`             |
| `15th of month`         | `FREQ=MONTHLY;BYMONTHDAY=15`            |
| `1st monday of month`   | `FREQ=MONTHLY;BYDAY=1MO`                |
| `last friday of month`  | `FREQ=MONTHLY;BYDAY=-1FR`               |
| `yearly`                | `FREQ=YEARLY`                            |

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

/// Serializable weekday (jiff::civil::Weekday is not Serialize).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SerdeWeekday {
    Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday,
}

impl From<SerdeWeekday> for jiff::civil::Weekday { ... }
impl From<jiff::civil::Weekday> for SerdeWeekday { ... }

/// Monthly recurrence anchor — mutually exclusive, so modelled as an enum.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonthlyAnchor {
    /// e.g. "1st of month" → day=1, "monthly" → day=dtstart.day()
    DayOfMonth { day: u8 },
    /// e.g. "1st monday of month" → nth=1, weekday=Monday
    NthWeekday { nth: u8, weekday: SerdeWeekday },
    /// e.g. "last friday of month"
    LastWeekday { weekday: SerdeWeekday },
}

/// Recurrence rule stored on the task.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recurrence {
    pub freq: Freq,
    /// For Weekly: which weekdays (empty = same weekday as dtstart).
    pub weekdays: Vec<SerdeWeekday>,
    /// For Monthly: how to anchor within the month.
    pub monthly_anchor: Option<MonthlyAnchor>,
    /// End date (inclusive). None = repeat forever.
    pub until: Option<Date>,
    /// Anchor date — the first occurrence, determines the recurrence rhythm.
    pub dtstart: Date,
}
```

`interval` and `count` are omitted — no supported pattern needs them.

### New `Task` fields

```rust
pub struct Task {
    // ... existing fields unchanged ...

    /// Recurrence rule. None = one-off task.
    #[serde(default)]
    pub recurrence: Option<Recurrence>,

    /// Dates of occurrences already completed.
    #[serde(default)]
    pub completed_occurrences: Vec<Date>,
}
```

`#[serde(default)]` means existing stored tasks deserialize correctly without any migration loop.

### Occurrence generator

Used only for range-based views (Upcoming):

```rust
/// Returns all occurrence dates in [dtstart, up_to] that have not yet been completed.
pub fn pending_occurrences_up_to(task: &Task, up_to: Date) -> Vec<Date>
```

### `is_pending_on` — O(1) direct check

Does **not** call `pending_occurrences_up_to`. Checks mathematically whether `date` is an occurrence, then checks `completed_occurrences`:

```rust
pub fn is_pending_on(task: &Task, date: Date) -> bool {
    let Some(rule) = &task.recurrence else { return false };
    if date < rule.dtstart { return false }
    if rule.until.is_some_and(|u| date > u) { return false }
    let is_occurrence = match &rule.freq {
        Freq::Daily => true,  // every day from dtstart
        Freq::Weekly => rule.weekdays.is_empty()
            // same weekday as dtstart
            ? date.weekday() == rule.dtstart.weekday()
            // any of the listed weekdays
            : rule.weekdays.iter().any(|w| date.weekday() == (*w).into()),
        Freq::Monthly => match rule.monthly_anchor.as_ref().unwrap() {
            MonthlyAnchor::DayOfMonth { day } => date.day() as u8 == *day,
            MonthlyAnchor::NthWeekday { nth, weekday } => is_nth_weekday_of_month(date, *nth, *weekday),
            MonthlyAnchor::LastWeekday { weekday } => is_last_weekday_of_month(date, *weekday),
        },
        Freq::Yearly => date.month() == rule.dtstart.month() && date.day() == rule.dtstart.day(),
    };
    is_occurrence && !task.completed_occurrences.contains(&date)
}
```

---

## Step 2: Recurrence Pattern Parser (`src/recurrence_parser.rs`)

New file — parses the `--every` CLI string into a `Recurrence`.

```rust
pub enum RecurrenceParseError {
    UnknownPattern(String),
}

/// `dtstart` comes from the task's scheduled date (or today).
pub fn parse_recurrence(input: &str, dtstart: Date) -> Result<Recurrence, RecurrenceParseError>
```

| Input | Result |
|-------|--------|
| `"daily"` | `freq=Daily, weekdays=[]` |
| `"weekly"` | `freq=Weekly, weekdays=[]` |
| `"monday"` | `freq=Weekly, weekdays=[Monday]` |
| `"mon,wed,fri"` | `freq=Weekly, weekdays=[Mon,Wed,Fri]` |
| `"monthly"` | `freq=Monthly, anchor=DayOfMonth { day: dtstart.day() }` |
| `"1st of month"` | `freq=Monthly, anchor=DayOfMonth { day: 1 }` |
| `"1st monday of month"` | `freq=Monthly, anchor=NthWeekday { nth:1, weekday:Monday }` |
| `"last friday of month"` | `freq=Monthly, anchor=LastWeekday { weekday:Friday }` |
| `"yearly"` | `freq=Yearly, weekdays=[]` |

`until` is always `None` here — set by the caller from the `--until` flag.

`Display` for `Recurrence` (used in UI badge):

```
Daily                    → "every day"
Weekly, weekdays=[]      → "every week"
Weekly, [Mon,Wed,Fri]    → "Mon, Wed, Fri"
Monthly DayOfMonth(1)    → "1st of month"
Monthly NthWeekday(1,Mo) → "1st Mon of month"
Monthly LastWeekday(Fr)  → "last Fri of month"
Yearly                   → "every year"
```

---

## Step 3: Storage Migration (`storage/migrations.rs`)

Because the new `Task` fields use `#[serde(default)]`, existing v6 data deserializes correctly with no field-adding loop. The migration only needs to bump the version:

```rust
fn migrate_v6_to_v7(value: &mut serde_json::Value) {
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
    /// Active tasks that have a recurrence rule and have not been stopped.
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

### 5b. `MoveTaskParameters`

```rust
pub struct MoveTaskParameters {
    // ... existing ...
    pub recurrence: Option<Recurrence>,
    pub clear_recurrence: bool,
}
```

### 5c. `CompleteTaskParameters` — add `stop` flag

```rust
pub struct CompleteTaskParameters {
    pub task_number_or_fuzzy_name: String,
    /// If true and the task is recurring, cancel it permanently (sets completed_at).
    pub stop: bool,
}
```

### 5d. `complete_task` — three code paths

```
non-recurring             → existing behavior: set completed_at = now
recurring + stop=false    → append occurrence date to completed_occurrences
recurring + stop=true     → set completed_at = now (stops the recurrence)
```

No new service function needed — the `stop` flag on `CompleteTaskParameters` covers the cancel case.

`CompleteTaskResult` is unchanged.

---

## Step 6: View rendering (`main.rs` / view handlers)

**Today view**: include recurring tasks where `is_pending_on(task, today)` is true.

**Upcoming view**: for each recurring task, call `pending_occurrences_up_to(task, end_of_window)` and inject a row per pending occurrence date.

**Inbox / Someday / Logbook**: recurring tasks never appear here.

**Recurring view** (`tdo view recurring`): show all non-stopped recurring tasks, one row each, with their next pending occurrence date and recurrence badge.

---

## Step 7: CLI (`main.rs`)

### New flags on `Add` and `Move`

```
--every <PATTERN>    Recurrence pattern ("daily", "monday", "mon,wed,fri", …)
--until <DATE>       Stop recurring after this date
--clear-recurrence   (Move only) Remove recurrence from the task
```

### Updated `done` flag

```
--stop    Cancel a recurring task permanently
```

### `tdo view recurring`

New `ViewEntity` variant — shows all recurring tasks.

---

## Step 8: UI (`ui.rs`)

In `render_task_line`, if `task.recurrence.is_some()`, append a dimmed recurrence badge:

```
42  ○  Team standup          Work  ↻ Mon, Wed, Fri   next: Mon Feb 23
43  ○  Pay rent           Personal  ↻ 1st of month   next: Sun Mar 1
```

---

## Step 9: Tests

### Unit tests in `models/task.rs`

- `is_pending_on` for each `Freq` variant
- `is_pending_on` returns false for completed occurrences
- `is_pending_on` respects `until`
- `is_pending_on` returns false before `dtstart`
- `pending_occurrences_up_to` edge cases: month-end clamping (Jan 31 → Feb 28/29), leap years

### Unit tests in `recurrence_parser.rs`

- All input strings parse to the correct `Recurrence`
- Case-insensitive and whitespace-tolerant
- Unknown strings return `RecurrenceParseError`

### Integration tests (`tests/recurring_tasks.rs`)

- `tdo add "standup" --every monday` → task created with recurrence
- `tdo view recurring` → task appears
- `tdo done <id>` → occurrence appended to `completed_occurrences`, task still active
- `tdo done <id> --stop` → `completed_at` set, task disappears from all views
- `tdo done <id>` when date > `until` → no-op / informative error
- v6 JSON loads cleanly without migration loop

---

## Files Changed Summary

| File | Change |
|------|--------|
| `crates/tdo/src/models/task.rs` | Add `SerdeWeekday`, `Freq`, `MonthlyAnchor`, `Recurrence`; add `recurrence` + `completed_occurrences` to `Task`; add `is_pending_on()` + `pending_occurrences_up_to()` |
| `crates/tdo/src/recurrence_parser.rs` | **New file** — parse `--every` strings into `Recurrence` |
| `crates/tdo/src/storage/migrations.rs` | Add `migrate_v6_to_v7` (version bump only), bump `CURRENT_VERSION` to 7 |
| `crates/tdo/src/models/store.rs` | Bump version to 7; add `get_recurring_tasks()` |
| `crates/tdo/src/services/tasks.rs` | Add `recurrence` to `AddTaskParameters`/`MoveTaskParameters`; add `stop` to `CompleteTaskParameters`; update `complete_task` with three code paths |
| `crates/tdo/src/main.rs` | Add `--every`, `--until`, `--clear-recurrence`, `--stop` flags; add `View::Recurring`; update view pipelines |
| `crates/tdo/src/ui.rs` | Add recurrence badge + next-occurrence date to `render_task_line` |
| `crates/tdo/src/lib.rs` | `mod recurrence_parser;` declaration |
| `tests/recurring_tasks.rs` | **New file** — integration tests |

---

## Implementation Order

1. `models/task.rs` — types + `is_pending_on` + `pending_occurrences_up_to`
2. `recurrence_parser.rs` — parser + unit tests
3. `storage/migrations.rs` + `models/store.rs` — version bump
4. `services/tasks.rs` — wire recurrence through add/move/complete
5. `main.rs` — CLI flags + view pipeline changes
6. `ui.rs` — recurrence badge
7. Integration tests
