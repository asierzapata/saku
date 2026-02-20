# Major UX Issues - TDO CLI

**Priority:** P1 (High)  
**Estimated Total Effort:** 3-4 weeks  

---

## Task 1: Add Confirmation for Fuzzy Match Operations

**Problem:** Fuzzy matching for `tdo done` and `tdo delete` can accidentally match wrong tasks and execute without confirmation

**Current Behavior:**
```bash
tdo add "Review PR"
tdo done "review"  # Completes immediately without asking!
```

**Risk:** Destructive operations (complete, delete) happen without user confirmation

**Solution:** Add confirmation prompt when using fuzzy match (non-ID parameter)

**Expected Behavior:**
```bash
tdo done "review"
Found: "Review PR" (#42)
Complete this task? [y/N]: _
```

**Implementation Details:**
- Detect if identifier is numeric (task number) or string (fuzzy match)
- For fuzzy matches, show matched task and ask for confirmation
- Add `--force` or `-f` flag to skip confirmation (for scripting)
- Only apply to destructive operations (done, delete)
- Non-destructive operations (move, edit) don't need confirmation

**Files to Change:**
- `crates/tdo/src/services/tasks.rs` - add confirmation logic
- `crates/tdo/src/main.rs` - add `--force` flag
- Tests: verify confirmation prompt works

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Add `--force` flag to `done` and `delete` commands
- [ ] Detect if identifier is task number or fuzzy string
- [ ] Add confirmation prompt function
- [ ] Show matched task details before confirming
- [ ] Skip confirmation if `--force` is provided
- [ ] Add tests for confirmation flow
- [ ] Update documentation

---

## Task 2: Implement Search Command

**Problem:** No way to search tasks by keyword in titles or notes

**Current Workaround:**
```bash
tdo all | grep "keyword"  # Loses formatting, not user-friendly
```

**Solution:** Add dedicated search command

**Expected Behavior:**
```bash
tdo search "backend"

SEARCH RESULTS (3 tasks)

  42  ○  Review backend PR                    Work / api-project
  51  ○  Deploy backend to staging            Work / api-project  
  67  ○  Fix backend auth bug                 Work / api-project
```

**Features:**
- Search in task titles (case-insensitive)
- Search in task notes
- Highlight matching terms (optional)
- Show which field matched (title vs notes)
- Support for multiple search terms (AND logic)

**Advanced Options:**
```bash
tdo search "backend" --in title          # Search only titles
tdo search "backend" --in notes          # Search only notes
tdo search "api" "auth" --all            # Must match all terms (AND)
tdo search "bug" "fix" --any             # Match any term (OR)
```

**Implementation Details:**
- Add `Search` subcommand to CLI
- Use case-insensitive substring matching
- Return tasks sorted by relevance (or task_number)
- Reuse existing UI rendering functions

**Files to Change:**
- `crates/tdo/src/main.rs` - add Search command
- `crates/tdo/src/services/` - add search service (optional, can be in main)
- `crates/tdo/src/models/store.rs` - add search methods
- Tests: search functionality

**Estimated Effort:** 6-8 hours

**Steps:**
- [ ] Add `Search { query: String, in_field: Option<String> }` command
- [ ] Implement search logic (title and notes)
- [ ] Add highlighting for matching terms (optional)
- [ ] Sort results by relevance
- [ ] Add tests for search scenarios
- [ ] Update documentation
- [ ] Add to shell completions

---

## Task 3: Add Filter Flags to View Commands

**Problem:** Can't filter views by project, area, or tag

**Current Workaround:** Must view entire list and manually scan

**Solution:** Add filter flags to all view commands

**Expected Behavior:**
```bash
tdo view today --project work-api        # Only today tasks in project
tdo view today --area work               # Only work area tasks
tdo view today --tag urgent              # Only urgent tasks
tdo view all --project work-api --tag bug  # Multiple filters (AND)
```

**Implementation Details:**
- Add optional filter flags to view commands: `--project`, `--area`, `--tag`
- Apply filters before rendering
- Support multiple filters (AND logic)
- Show filter info in header: "Today (filtered: project=work-api)"

**Files to Change:**
- `crates/tdo/src/main.rs` - add filter flags to all view commands
- `crates/tdo/src/models/store.rs` - add filtered query methods
- Tests: verify filtering works

**Estimated Effort:** 8-10 hours

**Steps:**
- [ ] Add filter flags to Today, Inbox, Upcoming, Someday, All, Logbook views
- [ ] Implement filtering logic in store methods
- [ ] Update view headers to show active filters
- [ ] Handle fuzzy matching for project/area names
- [ ] Add tests for filter combinations
- [ ] Update documentation
- [ ] Update shell completions

---

## Task 4: Add Dedicated Deadlines View

**Problem:** Deadlines are secondary in UI, easy to miss. No way to see all tasks with deadlines sorted by due date

**Current Workaround:** Use `tdo all` and manually look for deadline badges

**Solution:** Add `tdo view deadlines` command

**Expected Behavior:**
```bash
tdo view deadlines

DEADLINES (5 tasks)

Overdue (2)
  42  ●  Submit expense report               Work / Admin        | ⚑ Feb 15
  51  ●  File quarterly taxes                Personal            | ⚑ Feb 16

This Week (2)
  67  ○  Review PR for auth                  Work / api-project  | ⚑ Feb 20
  73  ○  Prepare presentation                Work                | ⚑ Feb 22

Next Week (1)
  81  ○  Schedule dentist appointment        Personal            | ⚑ Feb 28
```

**Features:**
- Show all tasks with deadlines
- Group by urgency: Overdue / Today / This Week / Later
- Sort by deadline date (earliest first)
- Color coding: red (overdue), orange (today), yellow (this week)
- Include both incomplete and completed tasks (with filter option)

**Advanced Options:**
```bash
tdo view deadlines --upcoming    # Only future deadlines (no overdue)
tdo view deadlines --overdue     # Only overdue
tdo view deadlines --soon 7      # Deadlines within 7 days
```

**Implementation Details:**
- Add new view command for deadlines
- Filter tasks that have `deadline.is_some()`
- Group by time periods
- Reuse existing UI urgency colors

**Files to Change:**
- `crates/tdo/src/main.rs` - add Deadlines variant to view commands
- `crates/tdo/src/models/store.rs` - add deadline query methods
- `crates/tdo/src/ui.rs` - add deadline view rendering
- Tests: verify deadline grouping

**Estimated Effort:** 5-6 hours

**Steps:**
- [ ] Add `Deadlines` view command
- [ ] Implement deadline filtering and grouping
- [ ] Create deadline-specific rendering (grouped by urgency)
- [ ] Add color coding for urgency levels
- [ ] Add optional filters (upcoming, overdue, soon)
- [ ] Add tests
- [ ] Update documentation

---

## Task 5: Implement Bulk Operations

**Problem:** Can't operate on multiple tasks at once, tedious for common workflows

**Current Workaround:** Run command multiple times

**Solution:** Support multiple task IDs in commands

**Expected Behavior:**
```bash
# Complete multiple tasks
tdo done 42 51 67
✓ Completed 3 tasks:
  #42 Review PR
  #51 Deploy staging
  #67 Fix auth bug

# Delete multiple
tdo delete 10,11,12
✓ Deleted 3 tasks

# Move multiple tasks
tdo move 5-10 --today
✓ Moved 6 tasks to today

# Add tag to multiple
tdo move 42,51,67 --tag urgent
✓ Added tag 'urgent' to 3 tasks
```

**Supported Formats:**
- Space-separated: `tdo done 1 2 3`
- Comma-separated: `tdo done 1,2,3`
- Range: `tdo done 5-10` (inclusive)
- Mixed: `tdo done 1,2,5-10,15`

**Implementation Details:**
- Parse task number argument as string, split by delimiters
- Support ranges with `-` syntax
- Validate all task numbers exist before operating
- Show summary of operations (success count, failures)
- Option to continue on errors vs stop on first error

**Files to Change:**
- `crates/tdo/src/main.rs` - change task_number to Vec<String>
- `crates/tdo/src/services/tasks.rs` - add bulk operation functions
- Add task number parser utility
- Tests: verify bulk operations

**Estimated Effort:** 10-12 hours

**Steps:**
- [ ] Create task number parser (handles ranges, lists)
- [ ] Update command definitions to accept multiple IDs
- [ ] Implement bulk complete operation
- [ ] Implement bulk delete operation
- [ ] Implement bulk move/update operation
- [ ] Add progress/summary output
- [ ] Handle errors gracefully (partial success)
- [ ] Add `--continue-on-error` flag
- [ ] Add tests for all bulk scenarios
- [ ] Update documentation

---

## Task 6: Improve Deadline Badge Visibility

**Problem:** Deadline badges are small and easy to miss, especially for urgent deadlines

**Current Display:**
```
42  ○  Review PR                    Work / api-project  | ⚑ Feb 20
```

**Improved Display:**
```
# Overdue (red, bold)
42  ●  Review PR                    Work / api-project  | ⚑ OVERDUE Feb 15 ⚠

# Due today (orange, bold)
51  ○  Deploy staging               Work / api-project  | ⚑ DUE TODAY ⚠

# Approaching (yellow)
67  ○  Fix auth bug                 Work / api-project  | ⚑ Feb 20 (2 days)

# Normal (default)
73  ○  Write docs                   Work / api-project  | ⚑ Feb 25
```

**Improvements:**
- Add text labels for overdue/due today
- Add warning icon (⚠) for urgent deadlines
- Make deadline text bold when < 3 days
- Show relative time for approaching deadlines
- Color code by urgency (consistent with urgency system)

**Implementation Details:**
- Enhance `format_deadline_badge()` in ui.rs
- Add urgency calculation based on deadline
- Apply color and styling based on urgency
- Keep formatting concise (terminal width aware)

**Files to Change:**
- `crates/tdo/src/ui.rs` - enhance deadline formatting
- Tests: verify deadline display

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Update `format_deadline_badge()` function
- [ ] Add urgency-based styling (bold, colors)
- [ ] Add text labels (OVERDUE, DUE TODAY)
- [ ] Add relative time for approaching deadlines
- [ ] Test with various deadline scenarios
- [ ] Ensure fits in terminal width
- [ ] Update design spec if needed

---

## Task 7: Add Warning for Conflicting Command Flags

**Problem:** Some flag combinations don't make sense but aren't validated

**Examples of issues:**
```bash
# Conflicting scheduling
tdo add "task" --today --someday  # Which one wins?

# Move without destination
tdo move 42  # No-op? Should warn.

# Clear and set same field
tdo move 42 --clear-deadline --deadline 2026-03-01  # Conflict
```

**Solution:** Validate flag combinations and show helpful errors

**Expected Behavior:**
```bash
tdo add "task" --today --someday
Error: Cannot use both --today and --someday
Choose one scheduling option:
  --today       Schedule for today
  --someday     Defer to someday
  --on <date>   Schedule for specific date

tdo move 42
Error: No changes specified
Provide at least one option to update:
  --today / --someday / --on <date>  (scheduling)
  --project / --area                 (organization)
  --tag                              (tagging)
  --deadline / --clear-deadline      (deadlines)
```

**Implementation Details:**
- Add validation logic before calling service functions
- Check for mutually exclusive flags
- Check for required parameters
- Provide helpful error messages with examples

**Files to Change:**
- `crates/tdo/src/main.rs` - add validation before service calls
- Tests: verify validation works

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Identify all mutually exclusive flag combinations
- [ ] Add validation functions
- [ ] Create helpful error messages
- [ ] Add suggestions for correct usage
- [ ] Ensure exit code 2 for validation errors (not 1)
- [ ] Add tests for all validation scenarios
- [ ] Update documentation with examples

---

## Task 8: Better Error Messages with Suggestions

**Problem:** Current error messages are minimal, don't guide users to solutions

**Current:**
```bash
tdo done "nonexistent"
Error: Task 'nonexistent' not found
```

**Improved:**
```bash
tdo done "nonexistent"
Error: Task 'nonexistent' not found

Did you mean one of these?
  • Review PR (#42)
  • Fix bug (#51)

Or use 'tdo view all' to see all tasks.
```

**Solution:** Add fuzzy suggestions when operations fail

**Features:**
- Suggest similar task titles (Levenshtein distance)
- Suggest similar project/area names
- Show helpful next steps
- Include relevant commands to try

**Implementation Details:**
- Add fuzzy string matching utility
- Enhance all error messages with suggestions
- Keep suggestions concise (max 3-5)
- Add "learn more" links to docs

**Files to Change:**
- `crates/tdo/src/services/` - enhance all error returns
- Add string similarity utility
- Tests: verify suggestions appear

**Estimated Effort:** 6-8 hours

**Steps:**
- [ ] Add string similarity library (e.g., `strsim`)
- [ ] Create suggestion helper functions
- [ ] Update all "not found" errors with suggestions
- [ ] Add contextual help messages
- [ ] Test with common typos and mistakes
- [ ] Ensure suggestions don't clutter output
- [ ] Update error handling throughout codebase

---

## Priority Ranking (Suggested Order)

1. **Confirmation for fuzzy matches** (5 hours) - Safety issue, quick win
2. **Search command** (8 hours) - High user impact, frequently requested
3. **Filter flags on views** (10 hours) - Makes large task lists usable
4. **Deadlines view** (6 hours) - Important missing functionality
5. **Bulk operations** (12 hours) - Big efficiency improvement
6. **Better error messages** (8 hours) - Improves overall UX
7. **Deadline badge visibility** (4 hours) - Polish, can be done anytime
8. **Flag validation** (5 hours) - Nice to have, lower priority

**Total Effort:** 58 hours (~2 weeks with testing/docs)

**Quick Wins (Do First):**
- Confirmation for fuzzy matches (prevents accidents)
- Search command (immediately useful)
- Deadlines view (fills major gap)

---

## Success Metrics

After implementing these tasks, users should be able to:
- ✅ Search tasks quickly without piping to grep
- ✅ Filter views to focus on specific projects/areas
- ✅ Complete/delete multiple tasks efficiently
- ✅ See all deadlines in one place, sorted by urgency
- ✅ Avoid accidental completions/deletions with confirmations
- ✅ Get helpful error messages with suggestions
- ✅ Spot urgent deadlines at a glance

---

## Testing Requirements

Each task must include:
- [ ] Unit tests for core logic
- [ ] Integration tests for CLI commands
- [ ] Manual testing with real workflows
- [ ] Documentation with examples
- [ ] Help text updates
