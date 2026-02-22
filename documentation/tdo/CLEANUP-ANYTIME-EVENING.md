# Cleanup Task: Remove Deprecated "Anytime" and "Evening" Features

**Status:** DONE ✅
**Priority:** P0 (Documentation consistency)
**Estimated Effort:** 2-3 hours
**Completed:** Code-side `evening` field removed from `When::Scheduled`. Docs updated — no remaining references in user-facing docs.

---

## Background

The "anytime" and "evening" scheduling features were deprecated but references remain throughout the codebase and documentation. These need to be cleaned up to avoid user confusion.

**Current state:**
- `LegacyAnytime` and `LegacyToday { evening: bool }` exist ONLY for migration support
- No way to create new tasks with these modes
- Documentation incorrectly suggests these features are available

---

## Part 1: Documentation Cleanup - "Anytime" References

### Files to Update

#### 1. `/crates/tdo/README.md`
**Line 7:** Remove "Anytime" from feature list
```markdown
- **Flexible scheduling**: Today, Inbox, Upcoming, Anytime, Someday, or Logbook
```
Change to:
```markdown
- **Flexible scheduling**: Inbox, Today (scheduled dates), Upcoming, Someday, or Logbook
```

**Line 27:** Remove `tdo anytime` command
```markdown
tdo anytime         # Tasks to do anytime
```
Delete this line entirely.

---

#### 2. `/documentation/tdo/commands-cheat-sheet.md`

**Line 11:** Remove from Capture table
```markdown
| `tdo add "task" --anytime`             | Add to Anytime             |
```
Delete this row.

**Line 19:** Remove from note
```markdown
**Note:** Only one scheduling flag allowed: `--today`, `--someday`, `--anytime`, or `--when` (mutually exclusive)
```
Change to:
```markdown
**Note:** Only one scheduling flag allowed: `--today`, `--someday`, or `--when` (mutually exclusive)
```

**Line 29:** Remove from View table
```markdown
| `tdo anytime`        | No date, not someday          |
```
Delete this row.

**Line 67:** Remove from Move/Schedule table
```markdown
| `tdo move <id> --anytime`             | Move task to Anytime            |
```
Delete this row.

**Line 78:** Remove from notes
```markdown
- Only one scheduling flag allowed: `--today`, `--someday`, `--anytime`, or `--when` (mutually exclusive)
```
Change to:
```markdown
- Only one scheduling flag allowed: `--today`, `--someday`, or `--when` (mutually exclusive)
```

**Line 110:** Remove from Flags Reference table
```markdown
| `--anytime`         |       | Available anytime              |
```
Delete this row.

---

#### 3. `/documentation/tdo/testing.md`

**Line 34:** Remove from test coverage table
```markdown
| `tests/task_commands.rs` | `add` (inbox, today, someday, anytime, scheduled, with project/area/tags), `done`, `delete`, `restore`, `move` |
```
Change to:
```markdown
| `tests/task_commands.rs` | `add` (inbox, today, someday, scheduled, with project/area/tags), `done`, `delete`, `restore`, `move` |
```

**Line 35:** Remove from view commands test
```markdown
| `tests/view_commands.rs` | Default view, `today`, `inbox`, `upcoming`, `anytime`, `someday`, `logbook`, `trash`, `all`, `tag list`, `tag view` — both empty states and populated states |
```
Change to:
```markdown
| `tests/view_commands.rs` | Default view, `today`, `inbox`, `upcoming`, `someday`, `logbook`, `trash`, `all`, `tag list`, `tag view` — both empty states and populated states |
```

---

#### 4. `/crates/tdo/documentation/architecture.md`

**Line 175:** Remove from task scheduling list
```markdown
- `Anytime`: No specific date
```
Delete this line entirely.

---

#### 5. `/skills/saku-integration/SKILL.md`

**Line 36:** Remove from scheduling list
```markdown
- **Scheduling**: Inbox, Today, Upcoming, Someday, Anytime, Logbook
```
Change to:
```markdown
- **Scheduling**: Inbox, Today, Upcoming, Someday, Logbook
```

---

## Part 2: Documentation Cleanup - "Evening" References

### Files to Update

#### 1. `/documentation/tdo/commands-cheat-sheet.md`

**Line 9:** Remove evening flag from Capture table
```markdown
| `tdo add "task" --today --evening`     | Add to Today (evening tag) |
```
Delete this row (or update to just `--today` without evening).

**Line 65:** Remove evening flag from Move/Schedule table
```markdown
| `tdo move <id> --today --evening`     | Move task to Today (evening)    |
```
Delete this row (or update to just `--today` without evening).

**Line 108:** Remove from Flags Reference table
```markdown
| `--evening`         |       | Tag as evening (metadata only) |
```
Delete this row.

---

#### 2. `/crates/tdo/documentation/architecture.md`

**Line 177:** Remove evening mention
```markdown
- `Scheduled`: Specific date with optional evening flag
```
Change to:
```markdown
- `Scheduled`: Specific date
```

---

#### 3. `/documentation/tdo/design-spec.md`

**Line 18:** Remove "Evening" from view examples
```markdown
- Tasks are grouped under bold headers based on the current view (e.g., **Today**, **Upcoming**, **Evening**).
```
Change to:
```markdown
- Tasks are grouped under bold headers based on the current view (e.g., **Today**, **Upcoming**).
```

**Line 19:** Remove evening reference
```markdown
- A single empty line separates distinct groups (e.g., between the main "Today" list and the "Evening" bucket).
```
Change to:
```markdown
- A single empty line separates distinct groups (e.g., between different date sections).
```

**Line 98:** Remove evening section from mockup
```markdown
  ─── Evening ───

  6  ○  Read Chapter 5 of Rust book             Personal / Study
```
Delete these lines from the mockup example.

---

## Part 3: Code Cleanup - Remove `evening` field

### Files to Update

#### 1. `/crates/tdo/src/models/task.rs`

**Current:**
```rust
pub enum When {
    Inbox,
    Someday,
    Scheduled { date: Date, evening: Option<bool> },
    LegacyToday { evening: bool },
    LegacyAnytime,
}
```

**Change to:**
```rust
pub enum When {
    Inbox,
    Someday,
    Scheduled { date: Date },
    // Legacy variants for migration only
    LegacyToday { evening: bool },
    LegacyAnytime,
}
```

**Impact:** This is a breaking change to the data model.

---

#### 2. Create Migration v4 → v5

**New file:** `/crates/tdo/src/storage/migrations.rs`

Add a new migration function that converts old `Scheduled { date, evening: Some(_) }` to just `Scheduled { date }`:

```rust
fn migrate_v4_to_v5(store: &mut StoredStore) {
    store.version = 5;
    
    // Convert any Scheduled with evening field to just Scheduled
    for task in &mut store.tasks {
        if let When::Scheduled { date, evening: Some(_) } = task.when {
            task.when = When::Scheduled { date, evening: None };
        }
    }
}
```

Then update the migration chain to include v5.

---

#### 3. Update all code that constructs `When::Scheduled`

Search for: `When::Scheduled {`

**Files likely affected:**
- `/crates/tdo/src/models/task.rs` - `from_command_flags()` method
- `/crates/tdo/src/services/tasks.rs` - `add_task()`, `move_task()`
- `/crates/tdo/src/services/task_editor.rs` - parsing logic

**Change from:**
```rust
When::Scheduled { date: parsed_date, evening: None }
```

**Change to:**
```rust
When::Scheduled { date: parsed_date }
```

---

## Testing Checklist

After making changes, verify:

- [ ] All documentation renders correctly (no broken links/formatting)
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes all tests
- [ ] Migration v4→v5 works (test with real v4 data file)
- [ ] No compiler warnings about unused fields
- [ ] `tdo --help` doesn't mention evening/anytime
- [ ] All view commands work (`tdo today`, `tdo inbox`, etc.)
- [ ] Task creation works: `tdo add "test" --today`
- [ ] Task movement works: `tdo move 1 --today`

---

## Verification Commands

```bash
# Check for remaining references
cd /Users/asierzapata/Documents/Projects/saku
rg -i "anytime" --type md
rg -i "evening" --type md
rg "evening:" crates/tdo/src/

# Build and test
cd crates/tdo
cargo build
cargo test
cargo clippy

# Manual testing
tdo add "Test task" --today
tdo today
tdo inbox
```

---

## Estimated Time Breakdown

- Documentation updates: 30 minutes
- Code changes (remove evening field): 45 minutes
- Migration v5 implementation: 30 minutes
- Testing: 30 minutes
- Review and verification: 15 minutes

**Total: ~2.5 hours**

---

## Related Issues

- See `/documentation/tdo/qa-review-report.md` - Issue #1 and #3
- This cleanup is part of Phase 1 priority work

---

**Next Steps:**
1. Create a branch: `git checkout -b cleanup/remove-anytime-evening`
2. Make all documentation changes first (easy wins)
3. Make code changes and add migration
4. Test thoroughly
5. Create PR with this document as reference
