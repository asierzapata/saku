# Recurring Tasks Implementation Plan

## Overview

Add recurring task support to `tdo`. When a recurring task is completed, the next instance is automatically spawned for the next scheduled date. Recurrence metadata lives on the task itself (no separate template entity), keeping the model simple.

---

## Supported Recurrence Patterns

Based on the docs:

| User input              | Meaning                            |
|-------------------------|------------------------------------|
| `daily`                 | Every day                          |
| `weekly`                | Every 7 days from the task's date  |
| `monday`                | Every Monday                       |
| `mon,wed,fri`           | Every Mon, Wed, and Fri            |
| `monthly`               | Same day of month, every month     |
| `1st of month`          | 1st of every month                 |
| `15th of month`         | 15th of every month                |
| `1st monday of month`   | 1st Monday of every month          |
| `last friday of month`  | Last Friday of every month         |
| `yearly`                | Same date, every year              |

---

## Step 1: Data Model (`models/task.rs`)

Add two new types and two new fields to `Task`.

### New types

```rust
/// Describes when a recurring task repeats within a month
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonthlyAnchor {
    /// e.g. "1st of month" → day = 1, "15th of month" → day = 15
    DayOfMonth { day: u8 },
    /// e.g. "1st monday of month" → nth = 1, weekday = Monday
    NthWeekday { nth: u8, weekday: SerdeWeekday },
    /// e.g. "last friday of month"
    LastWeekday { weekday: SerdeWeekday },
}

/// Recurrence pattern
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecurrencePattern {
    Daily,
    Weekly,
    /// Specific weekdays — at least one element
    Weekdays { days: Vec<SerdeWeekday> },
    Monthly { anchor: MonthlyAnchor },
    Yearly,
}

/// Full recurrence config attached to a task
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recurrence {
    pub pattern: RecurrencePattern,
    /// Recurrence ends after this date (inclusive). None = repeat forever.
    pub until: Option<Date>,
}

/// jiff::civil::Weekday is not Serialize; wrap it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Copy)]
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
    // ... existing fields ...

    /// Recurrence configuration. None = one-off task.
    pub recurrence: Option<Recurrence>,

    /// UUID of the first task in this recurrence chain.
    /// None for non-recurring or the very first instance.
    pub recurring_origin_id: Option<Uuid>,
}
```

`Task::default()` already works via `#[derive(Default)]`; the new fields default to `None` / `vec![]`.

### Next-occurrence computation

Add a free function in `models/task.rs`:

```rust
/// Given a recurrence pattern and the date of the just-completed instance,
/// return the next scheduled date (or None if past `until`).
pub fn next_recurrence_date(
    recurrence: &Recurrence,
    completed_instance_date: Date,
) -> Option<Date>
```

Logic per pattern:
- **Daily** → `completed_instance_date + 1 day`
- **Weekly** → `completed_instance_date + 7 days`
- **Weekdays** → find the next weekday in the set that is strictly after `completed_instance_date` (wraps week)
- **Monthly / DayOfMonth** → same day next month (clamp to last day if needed)
- **Monthly / NthWeekday** → nth occurrence of that weekday in next month
- **Monthly / LastWeekday** → last occurrence of that weekday in next month
- **Yearly** → same date next year

After computing, check `recurrence.until`: if next date > until, return `None`.

---

## Step 2: Recurrence Pattern Parser (`src/recurrence_parser.rs`)

New file, analogous to `date_parser.rs`.

```rust
pub enum RecurrenceParseError {
    UnknownPattern(String),
}

/// Parse a user-supplied `--every` string into a `RecurrencePattern`.
pub fn parse_recurrence_pattern(input: &str) -> Result<RecurrencePattern, RecurrenceParseError>
```

Parsing rules (case-insensitive, trimmed):

| Input pattern                    | Result                                          |
|----------------------------------|-------------------------------------------------|
| `"daily"` / `"day"`              | `RecurrencePattern::Daily`                      |
| `"weekly"` / `"week"`            | `RecurrencePattern::Weekly`                     |
| `"monthly"` / `"month"`          | `RecurrencePattern::Monthly { anchor: DayOfMonth { day: 1 } }` (user can be specific) |
| `"yearly"` / `"year"`            | `RecurrencePattern::Yearly`                     |
| single weekday `"monday"` etc.   | `RecurrencePattern::Weekdays { days: [Monday] }` |
| comma-separated `"mon,wed,fri"`  | `RecurrencePattern::Weekdays { days: [...] }`   |
| `"1st of month"` / `"2nd of month"` … | `RecurrencePattern::Monthly { anchor: DayOfMonth { day: N } }` |
| `"1st monday of month"` etc.     | `RecurrencePattern::Monthly { anchor: NthWeekday { nth: 1, weekday: Monday } }` |
| `"last friday of month"` etc.    | `RecurrencePattern::Monthly { anchor: LastWeekday { weekday: Friday } }` |

Ordinal helpers: `"1st"→1`, `"2nd"→2`, `"3rd"→3`, `"4th"→4`, `"5th"→5`.

Include `Display` impl for `RecurrencePattern` (used in UI badge):
- `Daily` → `"every day"`
- `Weekdays([Mon, Wed, Fri])` → `"Mon, Wed, Fri"`
- `Monthly { DayOfMonth(1) }` → `"1st of month"`
- etc.

---

## Step 3: Storage Migration (`storage/migrations.rs`)

Add `migrate_v6_to_v7`:

```rust
fn migrate_v6_to_v7(value: &mut serde_json::Value) {
    // Add recurrence: null and recurring_origin_id: null to every task
    if let Some(tasks) = value["tasks"].as_array_mut() {
        for task in tasks.iter_mut() {
            task["recurrence"] = serde_json::Value::Null;
            task["recurring_origin_id"] = serde_json::Value::Null;
        }
    }
    value["version"] = serde_json::json!(7);
}
```

Bump `CURRENT_VERSION` constant from `6` to `7`. Add the new step to `apply_migrations`.

---

## Step 4: Store version bump (`models/store.rs`)

Change `StoredStore::version` default/constant from `6` to `7`. No other store changes needed — the new fields serialize/deserialize automatically via serde.

Also add a query helper for the recurring view:

```rust
impl Store {
    /// Return all active (not completed, not deleted) tasks that have recurrence set.
    pub fn get_recurring_tasks(&self) -> impl Iterator<Item = &Task> {
        self.get_active_tasks()
            .filter(|t| t.recurrence.is_some() && t.completed_at.is_none())
    }
}
```

---

## Step 5: Service layer (`services/tasks.rs`)

### 5a. `AddTaskParameters`

Add field:

```rust
pub struct AddTaskParameters {
    // ... existing ...
    pub recurrence: Option<Recurrence>,
}
```

In `add_task`, after building the `Task`, set `task.recurrence = parameters.recurrence`.

### 5b. `MoveTaskParameters`

Add optional field for updating recurrence:

```rust
pub struct MoveTaskParameters {
    // ... existing ...
    pub recurrence: Option<Recurrence>,       // set a new recurrence
    pub clear_recurrence: bool,               // remove recurrence
}
```

Apply in `move_task` similarly to how deadline is handled.

### 5c. `CompleteTaskResult` and `complete_task`

Update result type:

```rust
pub struct CompleteTaskResult {
    pub task: Task,
    pub newly_unblocked: Vec<Task>,
    pub next_recurring_instance: Option<Task>,   // NEW
}
```

In `complete_task`, after marking the task complete, add:

```rust
let next_recurring_instance = if let Some(recurrence) = &updated_task.recurrence {
    // Determine the completed instance's scheduled date (or today as fallback)
    let instance_date = match updated_task.when {
        When::Scheduled { date } => date,
        _ => jiff::Zoned::now().date(),
    };

    if let Some(next_date) = next_recurrence_date(recurrence, instance_date) {
        let mut next_task = Task {
            id: Uuid::new_v4(),
            task_number: 0,                             // assigned by store.add_task
            title: updated_task.title.clone(),
            notes: updated_task.notes.clone(),
            project_id: updated_task.project_id,
            area_id: updated_task.area_id,
            tags: updated_task.tags.clone(),
            when: When::Scheduled { date: next_date },
            deadline: None,                             // deadlines don't carry over
            defer_until: None,
            depends_on: vec![],
            checklist: updated_task.checklist.iter().map(|item| ChecklistItem {
                id: Uuid::new_v4(),
                title: item.title.clone(),
                completed: false,                       // reset checklist
            }).collect(),
            recurrence: Some(recurrence.clone()),
            recurring_origin_id: Some(
                updated_task.recurring_origin_id.unwrap_or(updated_task.id)
            ),
            completed_at: None,
            deleted_at: None,
            created_at: jiff::Timestamp::now(),
            modified_at: crate::sync_clock::next_modified_at(),
        };
        store.add_task(next_task);  // add_task assigns task_number
        Some(store.get_task(next_task.id).unwrap().clone())
    } else {
        None  // past `until` date
    }
} else {
    None
};

// persist
storage.save(store)?;

Ok(CompleteTaskResult {
    task: updated_task,
    newly_unblocked,
    next_recurring_instance,
})
```

---

## Step 6: CLI (`main.rs`)

### 6a. `Commands::Add` — new flags

```rust
Add {
    // ... existing flags ...
    /// Recurrence pattern, e.g. "daily", "monday", "mon,wed,fri", "1st of month"
    #[arg(long, value_name = "PATTERN")]
    every: Option<String>,

    /// End date for recurrence, e.g. "2026-12-31"
    #[arg(long, value_name = "DATE")]
    until: Option<String>,
}
```

Parsing in the `Add` arm:

```rust
let recurrence = if let Some(pattern_str) = every {
    let pattern = parse_recurrence_pattern(&pattern_str)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(2); })?;
    let until_date = until.map(|s| parse_natural_date(&s)
        .map_err(|e| { eprintln!("Error: {}", e); std::process::exit(2); })
        .unwrap());
    Some(Recurrence { pattern, until: until_date })
} else {
    None
};
```

Pass into `AddTaskParameters`.

### 6b. `Commands::Move` — new flags

Same `--every` / `--until` / `--clear-recurrence` flags. Pass into `MoveTaskParameters`.

### 6c. `Commands::View` — new `Recurring` variant

Add to the `ViewEntity` / view subcommand enum:

```rust
Recurring,
```

Handler:

```rust
ViewEntity::Recurring => {
    let store = load_store();
    let tasks: Vec<&Task> = store.get_recurring_tasks().collect();
    if tasks.is_empty() {
        println!("No recurring tasks.");
    } else {
        println!("  Recurring  ({} tasks)\n", tasks.len());
        for task in order_tasks(tasks) {
            render_task_line(task, &store);
        }
    }
}
```

### 6d. `Commands::Done` result handling

Update the done output to announce the spawned next instance:

```rust
if let Some(next) = result.next_recurring_instance {
    println!("  ↻ Next instance scheduled: #{} on {}", next.task_number, next.when_date_display());
}
```

---

## Step 7: UI (`ui.rs`)

### Recurrence badge in task lines

In `render_task_line`, after the context column, if `task.recurrence.is_some()`, append a dimmed recurrence indicator:

```
42  ○  Team standup                     Work  ↻ Mon, Wed, Fri
43  ○  Pay rent                         Personal  ↻ 1st of month
```

Use `↻` (or `⟳`) as the recurrence glyph, dimmed, followed by the pattern's display string.

---

## Step 8: Tests

### Unit tests in `models/task.rs`

- `next_recurrence_date` for each `RecurrencePattern` variant
- Edge cases: month-end clamping (Jan 31 → Feb 28/29), leap years, `until` boundary

### Unit tests in `recurrence_parser.rs`

- All supported input strings parse correctly
- Unknown strings return `RecurrenceParseError`
- Case-insensitive and whitespace-tolerant

### Integration tests in `tests/`

New file `tests/recurring_tasks.rs`:

- `tdo add "standup" --every monday --project work` → task created with recurrence
- `tdo view recurring` → shows the task
- `tdo done <id>` → original marked complete, new instance created with correct date
- `tdo done <id>` on last instance (past `--until`) → no next instance spawned
- `tdo move <id> --clear-recurrence` → recurrence removed
- Migration: v6 store JSON with no recurrence fields → loads correctly after migration

---

## Files Changed Summary

| File | Change |
|------|--------|
| `crates/tdo/src/models/task.rs` | Add `SerdeWeekday`, `MonthlyAnchor`, `RecurrencePattern`, `Recurrence` types; add `recurrence` + `recurring_origin_id` to `Task`; add `next_recurrence_date()` fn |
| `crates/tdo/src/recurrence_parser.rs` | **New file** — parse `--every` strings |
| `crates/tdo/src/storage/migrations.rs` | Add `migrate_v6_to_v7`, bump `CURRENT_VERSION` to 7 |
| `crates/tdo/src/models/store.rs` | Bump version to 7; add `get_recurring_tasks()` |
| `crates/tdo/src/services/tasks.rs` | Extend `AddTaskParameters`, `MoveTaskParameters`, `CompleteTaskResult`; update `complete_task` to spawn next instance |
| `crates/tdo/src/main.rs` | Add `--every`, `--until`, `--clear-recurrence` flags to `Add`/`Move`; add `View::Recurring` subcommand; print next-instance message on done |
| `crates/tdo/src/ui.rs` | Add recurrence badge (↻ + pattern string) to `render_task_line` |
| `crates/tdo/src/lib.rs` / `main.rs` | `mod recurrence_parser;` declaration |
| `tests/recurring_tasks.rs` | **New file** — integration tests |

---

## Suggested Implementation Order

1. `models/task.rs` — data types + `next_recurrence_date()`
2. `recurrence_parser.rs` — pattern parser + unit tests
3. `storage/migrations.rs` + `models/store.rs` — migration + version bump
4. `services/tasks.rs` — wire recurrence into add/move/complete
5. `main.rs` — CLI flags + `view recurring` + done output
6. `ui.rs` — recurrence badge
7. Integration tests
